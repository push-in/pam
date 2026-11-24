-- wrk script for scripts/soak.sh: rotates GET /ping, GET /headers with a
-- client X-Request-Id (often repeated) and one of 100 invented header names,
-- POST /echo with bodies from 0 to 16 KiB, and GET /async (Fiber suspension).
local counter = 0
local bodies = {}
for size = 0, 16 do
  bodies[size] = string.rep("x", size * 1024)
end

request = function()
  counter = counter + 1
  local kind = counter % 4
  if kind == 0 then
    return wrk.format("GET", "/ping")
  elseif kind == 1 then
    local headers = {}
    headers["X-Request-Id"] = "soak-" .. (counter % 512)
    headers["X-Soak-" .. (counter % 100)] = tostring(counter)
    return wrk.format("GET", "/headers", headers)
  elseif kind == 2 then
    local headers = {}
    headers["Content-Type"] = "application/octet-stream"
    return wrk.format("POST", "/echo", headers, bodies[counter % 17])
  end
  return wrk.format("GET", "/async")
end
