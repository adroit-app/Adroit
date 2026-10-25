-- What is offered after `@`, `/` or `$` is shown as it is typed by Neovim's
-- own completion, without putting it in the text until it is chosen. A
-- configuration that completes with a plugin of its own sets
-- `g:adroit_autocomplete` to false, and the plugin finds the offers as the
-- completion of the language server named adroit.
local offer = require("adroit.offer")
local server = require("adroit.server")

local M = {}

--- Whether Neovim's own completion shows what adroit offers.
function M.on()
  return vim.g.adroit_autocomplete ~= false
end

--- Show what the language server `client` offers in `buf` with Neovim's own
--- completion.
function M.enable(client, buf)
  vim.lsp.completion.enable(true, client.id, buf, {
    convert = function()
      return { kind = "" }
    end,
  })
  -- Asked on every change of a word that starts with one of them, not only
  -- as they are typed: Neovim's own trigger gives up on a character typed
  -- right after them, and nothing would offer again once nothing matched or
  -- a directory was chosen.
  vim.api.nvim_create_autocmd({ "TextChangedI", "TextChangedP" }, {
    buffer = buf,
    callback = function()
      if M.on() and offer.offers(vim.api.nvim_get_current_line():sub(1, vim.fn.col(".") - 1)) then
        vim.lsp.completion.get()
      end
    end,
  })
end

--- Show adroit's offers with Neovim's own completion once its language
--- server attaches, unless the configuration has turned that off by then.
function M.setup()
  vim.o.completeopt = "menuone,noinsert"
  vim.g.adroit_autocomplete = true
  vim.api.nvim_create_autocmd("LspAttach", {
    callback = function(ev)
      local client = vim.lsp.get_client_by_id(ev.data.client_id)
      if client and client.name == server.NAME and M.on() then
        M.enable(client, ev.buf)
      end
    end,
  })
end

return M
