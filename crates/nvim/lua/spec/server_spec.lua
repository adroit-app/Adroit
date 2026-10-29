local server = require("adroit.server")

local function offered(start, ...)
  return { start = start, items = { ... } }
end

local function o(word, abbr, menu, info)
  return { word = word, abbr = abbr, menu = menu, info = info or "" }
end

describe("a completion item", function()
  local position = { line = 3, character = 7 }

  it("puts the word over what is typed from where the offer starts", function()
    local got = server.items(offered(6, o("auth/web ", "auth/web", "session")), "see @au", position)
    eq(true, got.isIncomplete)
    eq({
      label = "auth/web",
      filterText = "au",
      sortText = "00001",
      detail = "session",
      textEdit = {
        range = { start = { line = 3, character = 5 }, ["end"] = position },
        newText = "auth/web ",
      },
    }, got.items[1])
  end)

  it("keeps the order offered, and what a command does as its documentation", function()
    local got = server.items(
      offered(1, o("/review ", "/review", "Review code"), o("/pr ", "/pr", "", "[pr]")),
      "/r",
      { line = 0, character = 2 }
    )
    eq({ "00001", "00002" }, { got.items[1].sortText, got.items[2].sortText })
    eq(nil, got.items[1].documentation)
    eq("[pr]", got.items[2].documentation)
  end)
end)

describe("the language server", function()
  local function client(complete)
    local exited
    local rpc = server.serve(complete or function()
      return { items = {} }
    end)({
      on_exit = function(code)
        exited = code
      end,
    })
    local function request(method, params)
      local got
      rpc.request(method, params, function(err, result)
        got = { err = err, result = result }
      end)
      wait(function()
        return got ~= nil
      end, "answered " .. method)
      return got
    end
    return rpc, request, function()
      return exited
    end
  end

  it("asks to be asked after an at sign, a slash or a dollar, by bytes", function()
    local _, request = client()
    local caps = request("initialize", {}).result.capabilities
    eq({ "@", "/", "$" }, caps.completionProvider.triggerCharacters)
    eq("utf-8", caps.positionEncoding)
  end)

  it("answers completion with what it is given", function()
    local _, request = client(function(params)
      return { items = { { label = params.position.character } } }
    end)
    eq({ items = { { label = 4 } } }, request("textDocument/completion", { position = { line = 0, character = 4 } }).result)
  end)

  it("refuses what it does not serve", function()
    local _, request = client()
    eq(-32601, request("textDocument/hover", {}).err.code)
  end)

  it("can answer completion asynchronously after its host replies", function()
    local _, request = client(function(_, done)
      vim.defer_fn(function()
        done({ items = { { label = "host-file" } } })
      end, 10)
    end)
    eq("host-file", request("textDocument/completion", {}).result.items[1].label)
  end)

  it("ends when told to exit", function()
    local rpc, _, exited = client()
    rpc.notify("exit")
    eq(0, exited())
    eq(true, rpc.is_closing())
  end)

  it("attaches to a buffer by its name and serves what is offered at the cursor", function()
    local buf = vim.api.nvim_get_current_buf()
    vim.api.nvim_buf_set_lines(buf, 0, -1, false, { "see @au" })
    server.start(buf, function(before)
      return offered(6, o(before, before, "seen"))
    end)
    local found
    wait(function()
      found = vim.lsp.get_clients({ name = server.NAME, bufnr = buf })[1]
      return found and found.initialized
    end, "attached")
    local got = vim.lsp.buf_request_sync(buf, "textDocument/completion", {
      textDocument = { uri = vim.uri_from_bufnr(buf) },
      position = { line = 0, character = 7 },
    }, 2000)
    eq("see @au", got[found.id].result.items[1].textEdit.newText)
  end)
end)
