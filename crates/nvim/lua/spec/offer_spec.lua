local offer = require("adroit.offer")

local function words(offered)
  return vim.tbl_map(function(i)
    return i.word
  end, offered.items)
end

local function c(word, menu, info)
  return { word = word, menu = menu, info = info or "" }
end

--- A worktree holding `src/lib.rs`, `src/main.rs`, `README.md` and a hidden
--- file.
local function worktree()
  local dir = vim.fn.resolve(vim.fn.tempname())
  vim.fn.mkdir(dir .. "/src", "p")
  for _, f in ipairs({ "src/lib.rs", "src/main.rs", "README.md", ".env" }) do
    vim.fn.writefile({}, dir .. "/" .. f)
  end
  return dir
end

describe("after an at sign", function()
  local candidates = { c("auth/web", "session"), c("src/auth.rs", "file"), c("README.md", "file") }

  it("offers the candidates holding what follows, from after the at sign", function()
    local offered = offer.at("see @au", { candidates = candidates })
    eq(6, offered.start)
    eq({ "auth/web ", "src/auth.rs " }, words(offered))
  end)

  it("matches whatever the case", function()
    eq({ "README.md " }, words(offer.at("@read", { candidates = candidates })))
  end)

  it("offers nothing inside a word", function()
    eq(nil, offer.at("mail@au", { candidates = candidates }))
  end)

  it("offers the sessions, then the worktree's entries, without hidden ones", function()
    local offered = offer.at("@", { candidates = { c("fix", "session") }, root = worktree() })
    eq({ "fix ", "src/", "README.md " }, words(offered))
  end)

  it("offers a directory's entries after its slash", function()
    eq({ "src/main.rs " }, words(offer.at("@src/m", { root = worktree() })))
  end)

  it("walks from the root of the disk after an at sign and a slash", function()
    local dir = worktree()
    eq({ dir .. "/src/" }, words(offer.at("@" .. dir .. "/sr", {})))
  end)

  it("walks nothing without a root", function()
    eq({}, words(offer.at("@sr", {})))
  end)

  it("finds a tracked file by any part of its path, and an entry and a file once", function()
    local from = { candidates = { c("README.md", "file"), c("src/main.rs", "file") }, root = worktree() }
    eq({ "src/main.rs " }, words(offer.at("@ma", from)))
    eq({ "README.md ", "src/main.rs " }, words(offer.at("@R", from)))
  end)

  it("offers each kind up to its limit", function()
    local many = {}
    for i = 1, offer.LIMIT + 5 do
      many[i] = c("s" .. i, "session")
    end
    eq(offer.LIMIT, #offer.at("@s", { candidates = many }).items)
  end)

  it("never asks for the commands", function()
    eq(false, offer.at("@", {}).wants_commands)
  end)
end)

describe("after a slash", function()
  local commands = {
    c("/compact", "Clear history but keep a summary"),
    c("/pr-review", "Review a pull request", "[pr]"),
    c("/review", "Review code"),
    c("$deploy", "Deploy the service"),
  }

  it("offers the commands beginning with what follows before those holding it", function()
    local offered = offer.at("/re", { commands = commands })
    eq(1, offered.start)
    eq({ "/review ", "/pr-review " }, words(offered))
  end)

  it("keeps what a command does and its hint", function()
    local item = offer.at("/pr", { commands = commands }).items[1]
    eq({ word = "/pr-review ", abbr = "/pr-review", menu = "Review a pull request", info = "[pr]" }, item)
  end)

  it("offers the commands after a space too, and none inside a word", function()
    eq(6, offer.at("then /r", { commands = commands }).start)
    eq(nil, offer.at("and/r", { commands = commands }))
  end)

  it("offers a skill, put in as a dollar invocation", function()
    eq({ "$deploy " }, words(offer.at("/dep", { commands = commands })))
  end)

  it("asks for the commands while nothing follows it", function()
    eq(true, offer.at("/", { commands = commands }).wants_commands)
    eq(false, offer.at("/r", { commands = commands }).wants_commands)
  end)
end)

describe("after a dollar", function()
  it("offers the skills alone", function()
    local commands = { c("/deploy-docs", ""), c("$deploy", "") }
    eq({ "$deploy " }, words(offer.at("$dep", { commands = commands })))
  end)
end)

describe("a word offered for", function()
  it("starts with an at sign, a slash or a dollar, at the start or after a space", function()
    for _, before in ipairs({ "@", "@src/ma", "x /re", "$d" }) do
      eq(true, offer.offers(before), before)
    end
    for _, before in ipairs({ "", "mail@x", "and/or", "x" }) do
      eq(false, offer.offers(before), before)
    end
  end)
end)
