-- What the prompt offers as it is written. `@` at the start of a word offers
-- the sessions and files in `from.candidates` and the entries of the
-- directory typed, walked from `from.root`; `/` there offers the commands in
-- `from.commands`, and `$` the skills among them.
local M = {}

-- How many of each kind are offered at most.
M.LIMIT = 200

local function word_start(before, pattern)
  local s = before:find(pattern)
  if not s or (s > 1 and not before:sub(s - 1, s - 1):match("%s")) then
    return nil
  end
  return s, before:sub(s + 1)
end

local function item(c, word)
  return { word = word, abbr = c.word, menu = c.menu, info = c.info or "" }
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

local function paths(typed, root, out, seen)
  local cut = typed:match(".*()/")
  local head = cut and typed:sub(1, cut) or ""
  local want = typed:sub(#head + 1):lower()
  local dir
  if head:sub(1, 1) == "/" then
    dir = head
  elseif root and root ~= "" then
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
      out[#out + 1] = { word = e.dir and path or path .. " ", abbr = path, menu = e.dir and "dir" or "file", info = "" }
      n = n + 1
      if n >= M.LIMIT then
        return
      end
    end
  end
end

local function offer_paths(typed, from)
  local want = typed:lower()
  local sessions, files, seen = {}, {}, {}
  for _, c in ipairs(from.candidates or {}) do
    if c.word:lower():find(want, 1, true) then
      local list = c.menu == "file" and files or sessions
      if #list < M.LIMIT then
        list[#list + 1] = c
      end
    end
  end
  local out = {}
  for _, c in ipairs(sessions) do
    out[#out + 1] = item(c, c.word .. " ")
  end
  paths(typed, from.root, out, seen)
  for _, c in ipairs(files) do
    if not seen[c.word] then
      out[#out + 1] = item(c, c.word .. " ")
    end
  end
  return out
end

local function offer_commands(typed, sigil, from)
  local want = typed:lower()
  local first, then_ = {}, {}
  for _, c in ipairs(from.commands or {}) do
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
  return first
end

--- What is offered for `before`, the line up to the cursor, from the
--- candidates, commands and root in `from`; nil where nothing is offered.
--- `start` is the byte the text chosen goes from, counted from 1, and
--- `wants_commands` says a bare sigil asks the window for the commands anew.
function M.at(before, from)
  local s, typed = word_start(before, "@[^%s@]*$")
  if s then
    return { start = s + 1, items = offer_paths(typed, from), wants_commands = false }
  end
  for _, sigil in ipairs({ "/", "$" }) do
    local pattern = "%" .. sigil .. "[^%s%" .. sigil .. "]*$"
    s, typed = word_start(before, pattern)
    if s then
      return {
        start = s,
        items = offer_commands(typed, sigil, from),
        wants_commands = typed == "",
      }
    end
  end
  return nil
end

--- Whether `before`, the line up to the cursor, is in a word that is offered
--- for: one starting with `@`, `/` or `$`.
function M.offers(before)
  return before:match("^[@/$]%S*$") ~= nil or before:match("%s[@/$]%S*$") ~= nil
end

return M
