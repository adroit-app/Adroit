local keys = require("adroit.keys")

local function told()
  local seen = {}
  vim.rpcnotify = function(chan, name, ...)
    seen[#seen + 1] = { chan, name, ... }
  end
  return seen
end

describe("adroit's actions in the prompt", function()
  vim.g.adroit_channel = 7
  keys.setup()

  it("are asked of the window by :Adroit, as a binding names them", function()
    local seen = told()
    vim.cmd("Adroit console.send Escape")
    eq({ { 7, "adroit_action", "console.send Escape" } }, seen)
  end)

  it("are put on their keys in insert and normal mode", function()
    for _, k in ipairs(keys.KEYS) do
      for _, mode in ipairs({ "i", "n" }) do
        local m = vim.fn.maparg(k[1], mode, false, true)
        eq(false, vim.tbl_isempty(m), mode .. " " .. k[1])
        eq(k[4], m.desc, mode .. " " .. k[1])
      end
    end
  end)

  it("leave a key moving through completions to Neovim while it shows them", function()
    local pick = keys.unless_completing("<C-n>", "prompt.history_next")
    local pumvisible = vim.fn.pumvisible
    vim.fn.pumvisible = function()
      return 1
    end
    eq("<C-n>", pick())
    vim.fn.pumvisible = function()
      return 0
    end
    eq("<Cmd>Adroit prompt.history_next<CR>", pick())
    vim.fn.pumvisible = pumvisible
  end)

  it("give way to a configuration's own mapping of the same key", function()
    vim.keymap.set("i", "<C-p>", "mine")
    eq("mine", vim.fn.maparg("<C-p>", "i"))
  end)
end)
