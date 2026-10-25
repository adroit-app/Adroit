-- What adroit suggests for the prompt. It runs before the user's
-- configuration, which may change any of it.
local o = vim.o
o.showmode = false
o.ruler = false
o.shortmess = o.shortmess .. "IcF"
o.number = false
o.relativenumber = false
o.signcolumn = "no"
o.foldcolumn = "0"
o.statuscolumn = ""
o.cursorline = false
o.wrap = true
o.linebreak = true
vim.opt.fillchars:append({ eob = " " })
