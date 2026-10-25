-- adroit's actions, named as a key binding names them, run from the prompt
-- with `:Adroit prompt.send`, and the keys adroit puts them on, which a
-- configuration maps, or others, as it likes.
local M = {}

--- Ask the window for `action`, as `:Adroit` does.
function M.run(action)
  vim.rpcnotify(vim.g.adroit_channel, "adroit_action", action)
end

--- What a mapping types to run `action`.
function M.ask(action)
  return "<Cmd>Adroit " .. action .. "<CR>"
end

--- An expression mapping that leaves `key` to Neovim while it shows its
--- completions, which the keys that move through them belong to, and runs
--- `action` otherwise.
function M.unless_completing(key, action)
  return function()
    return vim.fn.pumvisible() == 1 and key or M.ask(action)
  end
end

-- The keys adroit puts its actions on, in insert and normal mode: each with
-- the action it runs, whether it stays Neovim's while completions show, and
-- what it does.
M.KEYS = {
  { "<C-CR>", "prompt.send", false, "Send the prompt" },
  { "<Tab>", "prompt.take_suggestion", true, "Take the session's suggested prompt into an empty prompt" },
  { "<S-Tab>", "console.send Shift+Tab", true, "Pass Shift+Tab to the session" },
  { "<C-c>", "console.send Escape", false, "Pass Escape to the session" },
  { "<C-p>", "prompt.history_prev", true, "Put the prompt sent before in the prompt" },
  { "<C-n>", "prompt.history_next", true, "Put the prompt sent after in the prompt" },
}

--- Make `:Adroit` and put adroit's actions on their keys.
function M.setup()
  vim.api.nvim_create_user_command("Adroit", function(cmd)
    M.run(cmd.args)
  end, { nargs = "+", desc = "Run adroit's action" })
  for _, k in ipairs(M.KEYS) do
    local lhs, action, completing, desc = k[1], k[2], k[3], k[4]
    if completing then
      vim.keymap.set({ "i", "n" }, lhs, M.unless_completing(lhs, action), { expr = true, desc = desc })
    else
      vim.keymap.set({ "i", "n" }, lhs, M.ask(action), { desc = desc })
    end
  end
end

return M
