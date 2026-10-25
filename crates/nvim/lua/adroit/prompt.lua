-- Turns the user's Neovim into a prompt editor: the frame around the text is
-- hidden, the buffer is a scratch buffer of its own filetype, what is offered
-- as it is written is served as a language server, and every text change
-- reports how many screen lines the text needs. Runs once startup has
-- finished, after the user's configuration, and holds what it sets against
-- any later change.
local hold = require("adroit.hold")
local offer = require("adroit.offer")
local server = require("adroit.server")

local M = {}

--- What the prompt offers for `before`, from what the window set, asking the
--- window on `chan` for the commands anew when a bare sigil is typed.
function M.offer(chan, before)
  local offered = offer.at(before, {
    candidates = vim.g.adroit_candidates,
    commands = vim.g.adroit_commands,
    root = vim.g.adroit_root,
  })
  if offered and offered.wants_commands then
    vim.rpcnotify(chan, "adroit_commands_wanted")
  end
  return offered
end

local function apply(chan)
  vim.cmd("silent! enew")
  local b = vim.api.nvim_get_current_buf()
  vim.g.adroit_buf = b
  local w = vim.api.nvim_get_current_win()
  hold.hold(function()
    return w
  end, b, {
    o = hold.everywhere(),
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
  require("adroit.config").watch()
  -- Told as Neovim quits, however it is quit, and waited for: what is
  -- written goes to the Neovim the window starts next.
  vim.api.nvim_create_autocmd("ExitPre", {
    callback = function()
      if vim.api.nvim_buf_is_valid(b) then
        local text = table.concat(vim.api.nvim_buf_get_lines(b, 0, -1, false), "\n")
        pcall(vim.rpcrequest, chan, "adroit_leaving", text, vim.fn.winsaveview())
      end
    end,
  })
  server.start(b, function(before)
    return M.offer(chan, before)
  end)
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

--- Set the prompt up for the window on `chan`, once startup has finished.
function M.setup(chan)
  vim.g.adroit_channel = chan
  if vim.v.vim_did_enter == 1 then
    apply(chan)
  else
    vim.api.nvim_create_autocmd("VimEnter", {
      once = true,
      callback = function()
        apply(chan)
      end,
    })
  end
end

return M
