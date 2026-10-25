-- The prompt's own buffer, shown again if `:e` or `:help` left it.
local b = vim.g.adroit_buf
if b and vim.api.nvim_buf_is_valid(b) and vim.api.nvim_get_current_buf() ~= b then
  vim.api.nvim_win_set_buf(0, b)
end
return vim.api.nvim_win_get_cursor(0)
