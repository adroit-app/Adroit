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
-- adroit's actions, named as a key binding names them, run from the prompt:
-- `:Adroit prompt.send`. The keys below are only where adroit puts them; a
-- configuration maps them, or others, as it likes.
vim.api.nvim_create_user_command("Adroit", function(cmd)
  vim.rpcnotify(vim.g.adroit_channel, "adroit_action", cmd.args)
end, { nargs = "+", desc = "Run adroit's action" })
local function ask(action)
  return "<Cmd>Adroit " .. action .. "<CR>"
end
-- While Neovim shows its completions, the keys that move through them stay
-- its own.
local function unless_completing(key, action)
  return function()
    return vim.fn.pumvisible() == 1 and key or ask(action)
  end
end
local map = vim.keymap.set
map({ "i", "n" }, "<C-CR>", ask("prompt.send"), { desc = "Send the prompt" })
map({ "i", "n" }, "<Tab>", unless_completing("<Tab>", "prompt.take_suggestion"), {
  expr = true,
  desc = "Take the session's suggested prompt into an empty prompt",
})
map({ "i", "n" }, "<S-Tab>", unless_completing("<S-Tab>", "console.send Shift+Tab"), {
  expr = true,
  desc = "Pass Shift+Tab to the session",
})
map({ "i", "n" }, "<C-c>", ask("console.send Escape"), { desc = "Pass Escape to the session" })
map({ "i", "n" }, "<C-p>", unless_completing("<C-p>", "prompt.history_prev"), {
  expr = true,
  desc = "Put the prompt sent before in the prompt",
})
map({ "i", "n" }, "<C-n>", unless_completing("<C-n>", "prompt.history_next"), {
  expr = true,
  desc = "Put the prompt sent after in the prompt",
})
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
