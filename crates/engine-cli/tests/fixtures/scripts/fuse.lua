-- ADR-0012 demo: uses engine.random_int to roll a countdown once (Fuse
-- starts at -1, the "not yet rolled" sentinel), then engine.despawn() to
-- remove itself when the countdown reaches zero.
function tick_fuse(components, tick, dt, self_id)
    local remaining = components.Fuse.ticks_remaining
    if remaining < 0 then
        remaining = engine.random_int(2, 4)
    else
        remaining = remaining - 1
    end
    if remaining <= 0 then
        engine.despawn()
        return nil
    end
    return { Fuse = { ticks_remaining = remaining } }
end
