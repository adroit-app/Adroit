-- What adroit suggests for the console's reader. It runs before the user's
-- configuration, which may change any of it.
local o = vim.o
o.showmode = false
o.ruler = false
o.shortmess = o.shortmess .. "IcF"
-- The window tells of a copy itself.
o.report = 99999
vim.opt.fillchars:append({ eob = " " })
