local url = require("adroit.url")

describe("the URL under the cursor", function()
  local line = "PR を出しました：https://github.com/a/b/pull/15 です"
  local from = line:find("https", 1, true)
  local to = line:find(" です", 1, true) - 1

  it("is the URL alone, though text other than ASCII touches it", function()
    eq("https://github.com/a/b/pull/15", url.at(line, from - 1))
    eq("https://github.com/a/b/pull/15", url.at(line, to - 1))
  end)

  it("is none off the URL", function()
    eq(nil, url.at(line, 0))
    eq(nil, url.at(line, to))
  end)

  it("leaves out the punctuation that ends the sentence around it", function()
    eq("https://a.dev/x", url.at("see https://a.dev/x.", 6))
    eq("https://a.dev/x", url.at("(https://a.dev/x)", 3))
    eq("https://a.dev/x_(y)", url.at("at https://a.dev/x_(y), then", 5))
  end)

  it("is the one the cursor is on, of several on a line", function()
    local two = "http://a.dev https://b.dev"
    eq("http://a.dev", url.at(two, 0))
    eq("https://b.dev", url.at(two, #two - 1))
  end)
end)
