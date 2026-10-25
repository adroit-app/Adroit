local complete = require("adroit.complete")
local server = require("adroit.server")

describe("Neovim's own completion", function()
  complete.setup()

  it("is suggested, showing what matches without putting it in the text", function()
    eq("menuone,noinsert", vim.o.completeopt)
    eq(true, complete.on())
  end)

  it("shows the language server's offers once it attaches", function()
    local buf = vim.api.nvim_create_buf(false, true)
    vim.api.nvim_set_current_buf(buf)
    server.start(buf, function()
      return nil
    end)
    wait(function()
      return #vim.api.nvim_get_autocmds({ event = "TextChangedI", buffer = buf }) > 0
    end, "enabled")
  end)

  it("is left to a configuration's own plugin once it turns it off", function()
    vim.g.adroit_autocomplete = false
    eq(false, complete.on())
    local buf = vim.api.nvim_create_buf(false, true)
    vim.api.nvim_set_current_buf(buf)
    server.start(buf, function()
      return nil
    end)
    local attached = vim.wait(500, function()
      return #vim.lsp.get_clients({ name = server.NAME, bufnr = buf }) > 0
    end, 10)
    eq(true, attached)
    eq(0, #vim.api.nvim_get_autocmds({ event = "TextChangedI", buffer = buf }))
    vim.g.adroit_autocomplete = true
  end)
end)
