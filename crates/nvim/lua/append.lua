local lines = ...
-- The prompt's own buffer, shown again if `:e` or `:help` left it.
local b = vim.g.adroit_buf
if not (b and vim.api.nvim_buf_is_valid(b)) then
  b = vim.api.nvim_get_current_buf()
elseif vim.api.nvim_get_current_buf() ~= b then
  vim.api.nvim_win_set_buf(0, b)
end
local cur = vim.api.nvim_buf_get_lines(b, 0, -1, false)
if #cur == 1 and cur[1] == "" then
  vim.api.nvim_buf_set_lines(b, 0, -1, false, lines)
  local n = vim.api.nvim_buf_line_count(b)
  vim.api.nvim_win_set_cursor(0, { n, #vim.api.nvim_buf_get_lines(b, n - 1, n, false)[1] })
else
  -- Lines added below the cursor leave it where it is, in what is written.
  vim.api.nvim_buf_set_lines(b, -1, -1, false, lines)
end
