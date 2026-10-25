
local b = vim.g.adroit_buf
if not (b and vim.api.nvim_buf_is_valid(b)) then
  b = 0
end
return table.concat(vim.api.nvim_buf_get_lines(b, 0, -1, false), "\n")
