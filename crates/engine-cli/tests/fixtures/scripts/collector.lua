-- ADR-0012 demo: uses engine.query to see every other entity with a
-- Position, and engine.despawn(id) to remove any close enough to "collect".
function collect(components, tick, dt, self_id)
    local nearby = engine.query({ "Position" })
    local cx = components.Position.x
    local cy = components.Position.y
    local cz = components.Position.z
    for _, e in ipairs(nearby) do
        if e.id ~= self_id then
            local dx = e.Position.x - cx
            local dy = e.Position.y - cy
            local dz = e.Position.z - cz
            local dist_sq = dx * dx + dy * dy + dz * dz
            if dist_sq < 1.0 then
                engine.despawn(e.id)
            end
        end
    end
    return nil
end
