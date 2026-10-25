
local lines, marks, cur, follow, background = ...
local b = vim.g.adroit_buf
if not (b and vim.api.nvim_buf_is_valid(b)) then
  return
end
if vim.api.nvim_get_current_buf() ~= b then
  vim.api.nvim_win_set_buf(0, b)
end
local ns = vim.api.nvim_create_namespace("adroit_console")
local was = vim.api.nvim_win_get_cursor(0)
local view = vim.fn.winsaveview()
vim.bo[b].modifiable = true
vim.api.nvim_buf_set_lines(b, 0, -1, false, lines)
vim.bo[b].modifiable = false
vim.api.nvim_buf_clear_namespace(b, ns, 0, -1)
vim.api.nvim_set_hl(0, "Normal", { bg = background, fg = vim.api.nvim_get_hl(0, { name = "Normal" }).fg })
for _, m in ipairs(marks) do
  local group = m[4]
  if vim.fn.hlexists(group) == 0 then
    vim.api.nvim_set_hl(0, group, {
      fg = m[5],
      bg = m[6] ~= "" and m[6] or nil,
      bold = m[7],
      italic = m[8],
      underline = m[9],
    })
  end
  pcall(vim.api.nvim_buf_set_extmark, b, ns, m[1], m[2], { end_col = m[3], hl_group = group })
end
if follow then
  vim.api.nvim_win_set_cursor(0, { cur[1] + 1, cur[2] })
else
  local n = vim.api.nvim_buf_line_count(b)
  vim.fn.winrestview(view)
  vim.api.nvim_win_set_cursor(0, { math.min(was[1], n), was[2] })
end
if vim.g.adroit_view then
  vim.g.adroit_view()
end
