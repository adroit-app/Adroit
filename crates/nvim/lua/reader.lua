
local chan = ...
vim.g.adroit_channel = chan
local function apply()
  local o = vim.o
  o.laststatus = 0
  o.showtabline = 0
  o.ruler = false
  o.showmode = false
  o.cmdheight = 0
  o.shortmess = o.shortmess .. "IcF"
  -- The window tells of a copy itself.
  o.report = 99999
  vim.cmd("silent! enew")
  local b = vim.api.nvim_get_current_buf()
  vim.g.adroit_buf = b
  vim.bo[b].buftype = "nofile"
  vim.bo[b].bufhidden = "hide"
  vim.bo[b].swapfile = false
  vim.bo[b].filetype = "adroit-console"
  vim.bo[b].modifiable = false
  local w = vim.api.nvim_get_current_win()
  vim.wo[w].number = false
  vim.wo[w].relativenumber = false
  vim.wo[w].signcolumn = "no"
  vim.wo[w].foldcolumn = "0"
  vim.wo[w].statuscolumn = ""
  vim.wo[w].wrap = false
  vim.wo[w].list = false
  vim.opt_local.fillchars:append({ eob = " " })
  -- The reader is for moving, searching and copying. Commands, Ex mode and
  -- quitting would leave it broken or gone, and windows and tabs arranged in
  -- it would stay. `<C-w>` takes the key after it too, or `<C-w>q` would
  -- start recording.
  for _, lhs in ipairs({ ":", "Q", "gQ", "ZZ", "ZQ" }) do
    vim.keymap.set({ "n", "x" }, lhs, "<Nop>")
  end
  vim.keymap.set({ "n", "x" }, "<C-w>", function()
    vim.fn.getcharstr()
  end)
  -- Keys are not the only way: a mapping or a plugin of the user's may open
  -- a window, as `K` opens help.
  vim.api.nvim_create_autocmd({ "WinNew", "TabNew" }, {
    callback = function()
      vim.schedule(function()
        if vim.api.nvim_win_is_valid(w) then
          vim.api.nvim_set_current_win(w)
        end
        vim.cmd("silent! tabonly! | silent! only!")
        if vim.api.nvim_buf_is_valid(b) then
          vim.api.nvim_win_set_buf(0, b)
        end
        w = vim.api.nvim_get_current_win()
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
if vim.v.vim_did_enter == 1 then
  apply()
else
  vim.api.nvim_create_autocmd("VimEnter", { once = true, callback = apply })
end
