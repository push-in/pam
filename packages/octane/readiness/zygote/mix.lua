-- Mixed Laravel traffic: DB+Redis, framework health, a short sleep.
local paths = {"/z/info", "/up", "/z/info", "/z/slow?ms=3"}
local n = 0
request = function()
  n = n + 1
  return wrk.format("GET", paths[(n % #paths) + 1])
end
