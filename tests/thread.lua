
assert(__file__():find 'thread.lua$')

local threads = {}
local tt = { n = 0 }
for i = 1, 64 do
    threads[i] = thread.spawn(function()
        tt.n = tt.n + 1
        print(tt.n)
    end)
end

for i, t in ipairs(threads) do
    t:join()
    print('#' .. i .. ' finished')
end

local cond = thread.condvar()
thread.spawn(function()
    print(cond:wait())
end)

thread.sleep(100)
cond:notify_one('notify: 111')