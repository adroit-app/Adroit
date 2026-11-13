-- The URL under the cursor, read from the characters a URL may hold.
-- `<cfile>`, which `gx` reads, takes every character beyond ASCII as part of
-- a file name, so text written up against a URL, as Japanese is, joins it.
local M = {}

local SCHEME = "%a[%w+.-]*://"
local BODY = "[%w%-%._~:/%?#%[%]@!%$&'%(%)%*%+,;=%%]*"
local TRAILING = "[%.,;:!%?']"

--- `text` without what ends the sentence around it: a trailing stop, comma
--- or quote, and a closing parenthesis that no opening one in it matches.
local function trimmed(text)
  while true do
    local last = text:sub(-1)
    if last:match(TRAILING) then
      text = text:sub(1, -2)
    elseif last == ")" and select(2, text:gsub("%(", "")) < select(2, text:gsub("%)", "")) then
      text = text:sub(1, -2)
    else
      return text
    end
  end
end

--- The URL of `line` that the byte at `col`, counted from 0, lies in; nil
--- when it lies in none.
function M.at(line, col)
  local init = 1
  while true do
    local from, to = line:find(SCHEME .. BODY, init)
    if not from then
      return nil
    end
    local found = trimmed(line:sub(from, to))
    if col + 1 >= from and col + 1 < from + #found then
      return found
    end
    init = to + 1
  end
end

--- The URL under the cursor of the current window, if any.
function M.under_cursor()
  local col = vim.api.nvim_win_get_cursor(0)[2]
  return M.at(vim.api.nvim_get_current_line(), col)
end

return M
