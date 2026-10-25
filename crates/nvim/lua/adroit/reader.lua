-- Turns the user's Neovim into the console's reader: no frame around the
-- text, lines kept unwrapped as the terminal shows them, and a yank told to
-- the window, which copies it. Runs after the user's configuration and holds
-- what it sets against any later change.
local hold = require("adroit.hold")

local M = {}

local function apply(chan)
  vim.cmd("silent! enew")
  local b = vim.api.nvim_get_current_buf()
  vim.g.adroit_buf = b
  local w = vim.api.nvim_get_current_win()
  -- The window lays the console's images over the text by the console's own
  -- rows and columns, so nothing may stand beside the text, wrap it or draw
  -- a character of it wider.
  hold.hold(function()
    return w
  end, b, {
    o = hold.everywhere(),
    bo = {
      buftype = "nofile",
      bufhidden = "hide",
      swapfile = false,
      filetype = "adroit-console",
      modifiable = false,
    },
    wo = {
      number = false,
      relativenumber = false,
      signcolumn = "no",
      foldcolumn = "0",
      statuscolumn = "",
      wrap = false,
      list = false,
      foldenable = false,
      conceallevel = 0,
      eventignorewin = "",
    },
  })
  -- The reader is for moving, searching and copying. Commands, Ex mode and
  -- quitting would leave it broken or gone, and windows and tabs arranged in
  -- it would stay. `<C-w>` takes the key after it too, or `<C-w>q` would
  -- start recording. They are the buffer's alone, not global, so that the
  -- buffers plugins open over it keep the user's own mappings; they are set
  -- after its filetype plugins', which would otherwise win over them.
  local function deny_all()
    local function deny(lhs, rhs)
      vim.keymap.set({ "n", "x" }, lhs, rhs, { buffer = b })
    end
    for _, lhs in ipairs({ ":", "Q", "gQ", "ZZ", "ZQ" }) do
      deny(lhs, "<Nop>")
    end
    deny("<C-w>", function()
      vim.fn.getcharstr()
    end)
  end
  deny_all()
  vim.api.nvim_create_autocmd("FileType", {
    buffer = b,
    callback = function()
      vim.schedule(deny_all)
    end,
  })
  require("adroit.config").watch()
  -- Keys are not the only way: a mapping or a plugin of the user's may open
  -- a window, as `K` opens help. Floating windows stay: they draw over the
  -- text, as plugins' pickers and notifications do, and leave it in place.
  local function floating(win)
    return vim.api.nvim_win_get_config(win).relative ~= ""
  end
  vim.api.nvim_create_autocmd({ "WinNew", "TabNew" }, {
    callback = function()
      vim.schedule(function()
        local was = vim.api.nvim_get_current_win()
        if vim.api.nvim_win_is_valid(w) then
          vim.api.nvim_set_current_tabpage(vim.api.nvim_win_get_tabpage(w))
        end
        vim.cmd("silent! tabonly!")
        if not vim.api.nvim_win_is_valid(w) then
          w = vim.tbl_filter(function(win)
            return not floating(win)
          end, vim.api.nvim_tabpage_list_wins(0))[1]
        end
        for _, win in ipairs(vim.api.nvim_tabpage_list_wins(0)) do
          if win ~= w and not floating(win) then
            pcall(vim.api.nvim_win_close, win, true)
          end
        end
        if vim.api.nvim_buf_is_valid(b) and vim.api.nvim_win_get_buf(w) ~= b then
          vim.api.nvim_win_set_buf(w, b)
        end
        if vim.api.nvim_win_is_valid(was) and floating(was) then
          vim.api.nvim_set_current_win(was)
        else
          vim.api.nvim_set_current_win(w)
        end
      end)
    end,
  })
  -- Where the text is scrolled to, so the window can put images over it.
  local function view()
    local v = vim.fn.winsaveview()
    vim.rpcnotify(chan, "adroit_scrolled", v.topline - 1, v.leftcol)
  end
  vim.g.adroit_view = view
  vim.api.nvim_create_autocmd({ "WinScrolled", "BufEnter", "VimResized" }, { callback = view })
  vim.api.nvim_create_autocmd("TextYankPost", {
    buffer = b,
    callback = function()
      local e = vim.v.event
      if e.operator == "y" and (e.regname == "" or e.regname == "+" or e.regname == "*") then
        local text = table.concat(e.regcontents, "\n")
        if e.regtype == "V" then
          text = text .. "\n"
        end
        vim.rpcnotify(chan, "adroit_yank", text, #e.regcontents)
      end
    end,
  })
end

--- Set the reader up for the window on `chan`, once startup has finished.
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
