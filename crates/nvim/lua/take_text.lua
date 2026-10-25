
-- The prompt's own buffer, shown again if `:e` or `:help` left it.
local b = vim.g.adroit_buf
if not (b and vim.api.nvim_buf_is_valid(b)) then
  b = vim.api.nvim_get_current_buf()
elseif vim.api.nvim_get_current_buf() ~= b then
  vim.api.nvim_win_set_buf(0, b)
end
local lines = vim.api.nvim_buf_get_lines(b, 0, -1, false)
vim.api.nvim_buf_set_lines(b, 0, -1, false, {})
-- With `virtualedit` letting it stay past the end of a line, the cursor would
-- otherwise wait where the text ended.
vim.api.nvim_win_set_cursor(0, { 1, 0 })
return table.concat(lines, "\n")
