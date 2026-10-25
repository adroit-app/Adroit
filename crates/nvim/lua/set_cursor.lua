local line, byte = ...
-- The prompt's own buffer, shown again if `:e` or `:help` left it.
local b = vim.g.adroit_buf
if b and vim.api.nvim_buf_is_valid(b) and vim.api.nvim_get_current_buf() ~= b then
  vim.api.nvim_win_set_buf(0, b)
end
local n = vim.api.nvim_buf_line_count(0)
line = math.max(1, math.min(line, n))
local len = #vim.api.nvim_buf_get_lines(0, line - 1, line, false)[1]
vim.api.nvim_win_set_cursor(0, { line, math.min(byte, len) })
