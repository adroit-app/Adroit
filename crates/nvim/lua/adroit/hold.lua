local M = {}
-- Sets what the window relies on, and sets it again whenever anything changes
-- it later: a mapping, a plugin, a filetype plugin. It is set again once that
-- change is done, as setting an option inside its own OptionSet breaks the
-- plugins that watch it. A value given as a function is asked each time, and
-- one that answers nil is left alone.
function M.hold(win, buf, settings)
  local function want(value)
    if type(value) == "function" then
      return value()
    end
    return value
  end
  local function set(opts, scope)
    for name, value in pairs(scope or {}) do
      local v = want(value)
      if v ~= nil and opts[name] ~= v then
        opts[name] = v
      end
    end
  end
  local function apply()
    set(vim.o, settings.o)
    local w = win()
    if vim.api.nvim_win_is_valid(w) then
      set(vim.wo[w], settings.wo)
    end
    if vim.api.nvim_buf_is_valid(buf) then
      set(vim.bo[buf], settings.bo)
    end
  end
  local names = {}
  for _, scope in pairs(settings) do
    for name in pairs(scope) do
      names[#names + 1] = name
    end
  end
  vim.api.nvim_create_autocmd("OptionSet", {
    pattern = names,
    callback = function()
      vim.schedule(apply)
    end,
  })
  vim.api.nvim_create_autocmd("BufWinEnter", { buffer = buf, callback = apply })
  apply()
end
-- What both Neovims rely on. Without a status line, a tab line, a window bar
-- or a command line, every row of the grid is the text's; without ignored
-- events, adroit's own autocommands run; and the colour scheme draws for the
-- background of the window's theme.
function M.everywhere()
  return {
    laststatus = 0,
    showtabline = 0,
    cmdheight = 0,
    winbar = "",
    eventignore = "",
    background = function()
      return vim.g.adroit_background
    end,
  }
end

return M
