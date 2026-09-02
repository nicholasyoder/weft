-- games/sandbox's first scripted gameplay: walk the ball near a pickup and
-- hold E to collect it. Proves ADR-0012/0013's engine.query/engine.despawn/
-- engine.key_held work inside the real live `play` loop, not just batch
-- test fixtures (see ADR-0013's "games/sandbox still has zero scripts"
-- consequence note — this closes that gap).
--
-- ADR-0016: also the first real use of engine.play_sound, fired the same
-- tick as the despawn so a collect always has an audible confirmation
-- (games/sandbox/assets-src/pickup.ogg, CC0, via Kenney — see
-- pickup-CC0.txt).
local COLLECT_RANGE_SQ = 1.2 * 1.2
local PICKUP_SOUND = "572963a19fd19711de1fa4eee4d5f503d7b28310d7dfe14b0201235c6c60507b"

function on_tick(components, tick, dt, self_id)
    if not engine.key_held("E") then
        return nil
    end

    local pos = components.Transform.position
    local players = engine.query({ "Transform", "PlayerControl" })
    for _, player in ipairs(players) do
        local ppos = player.Transform.position
        local dx = ppos[1] - pos[1]
        local dy = ppos[2] - pos[2]
        local dz = ppos[3] - pos[3]
        local dist_sq = dx * dx + dy * dy + dz * dz
        if dist_sq < COLLECT_RANGE_SQ then
            engine.play_sound(PICKUP_SOUND, 1.0)
            engine.despawn()
            return nil
        end
    end
    return nil
end
