
local chan = ...
vim.g.adroit_channel = chan
local function apply()
  vim.cmd("silent! enew")
  local b = vim.api.nvim_get_current_buf()
  vim.g.adroit_buf = b
  local w = vim.api.nvim_get_current_win()
  hold(function()
    return w
  end, b, {
    o = held_everywhere(),
    wo = { eventignorewin = "" },
    bo = {
      buftype = "nofile",
      bufhidden = "hide",
      swapfile = false,
      filetype = "adroit-prompt",
      modifiable = true,
      readonly = false,
    },
  })
  watch_configuration()
  -- `@` at the start of a word offers the sessions and files the window sets
  -- in g:adroit_candidates and the entries of the directory typed, walked from
  -- g:adroit_root; `/` there offers the commands in g:adroit_commands.
  local function word_start(before, pattern)
    local s = before:find(pattern)
    if not s or (s > 1 and not before:sub(s - 1, s - 1):match("%s")) then
      return nil
    end
    return s, before:sub(s + 1)
  end
  local LIMIT = 200
  local function item(c, word)
    return { word = word, abbr = c.word, menu = c.menu, info = c.info }
  end
  local function entries(dir)
    local ok, names = pcall(vim.fn.readdir, dir)
    if not ok then
      return {}
    end
    local out = {}
    for _, name in ipairs(names) do
      -- Hidden entries are left to be typed, as a shell's completion does.
      if name:sub(1, 1) ~= "." then
        out[#out + 1] = { name = name, dir = vim.fn.isdirectory(dir .. "/" .. name) == 1 }
      end
    end
    table.sort(out, function(x, y)
      if x.dir ~= y.dir then
        return x.dir
      end
      return x.name:lower() < y.name:lower()
    end)
    return out
  end
  local function paths(typed, out, seen)
    local cut = typed:match(".*()/")
    local head = cut and typed:sub(1, cut) or ""
    local want = typed:sub(#head + 1):lower()
    local root = vim.g.adroit_root or ""
    local dir
    if head:sub(1, 1) == "/" then
      dir = head
    elseif root ~= "" then
      dir = root .. "/" .. head
    else
      return
    end
    local n = 0
    for _, e in ipairs(entries(dir)) do
      if e.name:sub(1, #want):lower() == want then
        local path = head .. e.name .. (e.dir and "/" or "")
        seen[path] = true
        -- A directory takes no space after it, so completing goes on inside.
        out[#out + 1] = { word = e.dir and path or path .. " ", abbr = path, menu = e.dir and "dir" or "file" }
        n = n + 1
        if n >= LIMIT then
          return
        end
      end
    end
  end
  local function offer_paths(s, typed)
    local want = typed:lower()
    local sessions, files, seen = {}, {}, {}
    for _, c in ipairs(vim.g.adroit_candidates or {}) do
      if c.word:lower():find(want, 1, true) then
        local list = c.menu == "file" and files or sessions
        if #list < LIMIT then
          list[#list + 1] = c
        end
      end
    end
    local out = {}
    for _, c in ipairs(sessions) do
      out[#out + 1] = item(c, c.word .. " ")
    end
    paths(typed, out, seen)
    for _, c in ipairs(files) do
      if not seen[c.word] then
        out[#out + 1] = item(c, c.word .. " ")
      end
    end
    return s + 1, out
  end
  local function offer_commands(s, typed, sigil)
    if typed == "" then
      vim.rpcnotify(chan, "adroit_commands_wanted")
    end
    local want = typed:lower()
    local first, then_ = {}, {}
    for _, c in ipairs(vim.g.adroit_commands or {}) do
      if sigil ~= "$" or c.word:sub(1, 1) == "$" then
        local name = c.word:sub(2):lower()
        if name:sub(1, #want) == want then
          first[#first + 1] = item(c, c.word .. " ")
        elseif name:find(want, 1, true) then
          then_[#then_ + 1] = item(c, c.word .. " ")
        end
      end
    end
    vim.list_extend(first, then_)
    return s, first
  end
  -- What is offered for `before`, the line up to the cursor: the byte the
  -- text chosen goes from, counted from 1, and what to choose from.
  local function offer(before)
    local s, typed = word_start(before, "@[^%s@]*$")
    if s then
      return offer_paths(s, typed)
    end
    s, typed = word_start(before, "/[^%s/]*$")
    if s then
      return offer_commands(s, typed, "/")
    end
    s, typed = word_start(before, "%$[^%s$]*$")
    if s then
      return offer_commands(s, typed, "$")
    end
  end
  -- What is offered is served as a language server's completion, so that
  -- whichever completion the configuration runs, Neovim's own or a plugin's,
  -- shows it. Every answer is incomplete: what matches is found anew as more
  -- is typed, by any part of a path rather than by its start.
  local TRIGGERS = { "@", "/", "$" }
  local function items(params)
    local line = params.position.line
    local col = params.position.character
    local before = (vim.api.nvim_buf_get_lines(b, line, line + 1, false)[1] or ""):sub(1, col)
    local start, offered = offer(before)
    local out = {}
    for i, o in ipairs(offered or {}) do
      out[i] = {
        label = o.abbr,
        filterText = before:sub(start),
        sortText = string.format("%05d", i),
        detail = o.menu,
        documentation = o.info ~= "" and o.info or nil,
        textEdit = {
          range = { start = { line = line, character = start - 1 }, ["end"] = params.position },
          newText = o.word,
        },
      }
    end
    return { isIncomplete = true, items = out }
  end
  local function serve(dispatchers)
    local closing, id = false, 0
    local answers = {
      initialize = function()
        return {
          capabilities = {
            positionEncoding = "utf-8",
            completionProvider = { triggerCharacters = TRIGGERS },
          },
          serverInfo = { name = "adroit" },
        }
      end,
      ["textDocument/completion"] = items,
      shutdown = function()
        return vim.NIL
      end,
    }
    return {
      request = function(method, params, callback, notify_reply)
        id = id + 1
        local answer = answers[method]
        local mine = id
        vim.schedule(function()
          if answer then
            callback(nil, answer(params))
          else
            callback({ code = -32601, message = "adroit serves no " .. method })
          end
          if notify_reply then
            notify_reply(mine)
          end
        end)
        return true, mine
      end,
      notify = function(method)
        if method == "exit" then
          closing = true
          dispatchers.on_exit(0, 15)
        end
        return true
      end,
      is_closing = function()
        return closing
      end,
      terminate = function()
        closing = true
      end,
    }
  end
  vim.lsp.start({ name = "adroit", cmd = serve }, { bufnr = b })
  -- Told on every change, from wherever it comes: typed, pasted, or set by
  -- the window.
  local shell = false
  vim.api.nvim_buf_attach(b, false, {
    on_lines = function()
      vim.schedule(function()
        if not vim.api.nvim_buf_is_valid(b) then
          return
        end
        local now = (vim.api.nvim_buf_get_lines(b, 0, 1, false)[1] or ""):sub(1, 1) == "!"
        if now ~= shell then
          shell = now
          vim.rpcnotify(chan, "adroit_shell", now)
        end
      end)
    end,
  })
  local function report()
    local ok, h = pcall(vim.api.nvim_win_text_height, 0, {})
    local need = ok and h.all or vim.api.nvim_buf_line_count(0)
    -- A window grown to fit the text keeps the scroll it needed while it was
    -- smaller, hiding the first lines above an empty one.
    if need <= vim.api.nvim_win_get_height(0) and vim.fn.line("w0") > 1 then
      vim.fn.winrestview({ topline = 1 })
    end
    vim.rpcnotify(chan, "adroit_height", need)
  end
  local function view()
    vim.rpcnotify(chan, "adroit_view", vim.fn.line("w0") - 1, vim.api.nvim_buf_line_count(0))
  end
  vim.api.nvim_create_autocmd(
    { "TextChanged", "TextChangedI", "TextChangedP", "VimResized", "BufEnter", "WinResized" },
    { callback = report }
  )
  vim.api.nvim_create_autocmd(
    { "WinScrolled", "TextChanged", "TextChangedI", "BufEnter" },
    { callback = view }
  )
  report()
  vim.cmd("startinsert")
end
if vim.v.vim_did_enter == 1 then
  apply()
else
  vim.api.nvim_create_autocmd("VimEnter", { once = true, callback = apply })
end
