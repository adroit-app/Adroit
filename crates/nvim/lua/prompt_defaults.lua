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
-- What is offered after `@`, `/` or `$` is shown as it is typed by Neovim's
-- own completion, without putting it in the text until it is chosen. A
-- configuration that completes with a plugin of its own sets
-- `g:adroit_autocomplete` to false, and the plugin finds the offers as the
-- completion of the language server named adroit.
o.completeopt = "menuone,noinsert"
vim.g.adroit_autocomplete = true
vim.api.nvim_create_autocmd("LspAttach", {
  callback = function(ev)
    local client = vim.lsp.get_client_by_id(ev.data.client_id)
    if not (client and client.name == "adroit") or vim.g.adroit_autocomplete == false then
      return
    end
    vim.lsp.completion.enable(true, client.id, ev.buf, {
      convert = function()
        return { kind = "" }
      end,
    })
    -- Asked on every change of a word that starts with one of them, not only
    -- as they are typed: Neovim's own trigger gives up on a character typed
    -- right after them, and nothing would offer again once nothing matched
    -- or a directory was chosen.
    vim.api.nvim_create_autocmd({ "TextChangedI", "TextChangedP" }, {
      buffer = ev.buf,
      callback = function()
        if vim.g.adroit_autocomplete == false then
          return
        end
        local before = vim.api.nvim_get_current_line():sub(1, vim.fn.col(".") - 1)
        if before:match("^[@/$]%S*$") or before:match("%s[@/$]%S*$") then
          vim.lsp.completion.get()
        end
      end,
    })
  end,
})
