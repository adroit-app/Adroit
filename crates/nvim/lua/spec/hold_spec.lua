local hold = require("adroit.hold")

describe("what the window relies on", function()
  local win, buf = vim.api.nvim_get_current_win(), vim.api.nvim_get_current_buf()
  vim.g.answer = nil
  hold.hold(function()
    return win
  end, buf, {
    o = { laststatus = 0, background = function()
      return vim.g.answer
    end },
    wo = { wrap = false },
    bo = { buftype = "nofile" },
  })

  it("is set at once", function()
    eq({ 0, false, "nofile" }, { vim.o.laststatus, vim.wo[win].wrap, vim.bo[buf].buftype })
  end)

  it("is set again once anything changes it", function()
    vim.o.laststatus = 2
    vim.wo[win].wrap = true
    vim.bo[buf].buftype = ""
    wait(function()
      return vim.o.laststatus == 0 and not vim.wo[win].wrap and vim.bo[buf].buftype == "nofile"
    end, "set again")
  end)

  it("asks a value given as a function each time, leaving it alone while it answers nil", function()
    vim.o.background = "light"
    vim.o.laststatus = 2
    wait(function()
      return vim.o.laststatus == 0
    end, "set again")
    eq("light", vim.o.background)
    vim.g.answer = "dark"
    vim.o.laststatus = 2
    wait(function()
      return vim.o.background == "dark"
    end, "asked again")
  end)

  it("is what both Neovims need: no frame around the text and no event ignored", function()
    local all = hold.everywhere()
    eq({ 0, 0, 0, "", "" }, { all.laststatus, all.showtabline, all.cmdheight, all.winbar, all.eventignore })
  end)
end)
