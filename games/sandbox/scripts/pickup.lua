-- games/sandbox's first scripted gameplay: walk the ball near a pickup and
-- hold E to collect it. Proves ADR-0012/0013's engine.query/engine.despawn/
-- engine.key_held work inside the real live `play` loop, not just batch
-- test fixtures (see ADR-0013's "games/sandbox still has zero scripts"
-- consequence note — this closes that gap).
--
-- physics-substrate-plan.md Phase 6: proximity is now a real sensor overlap
-- (see the pickup entity's Collider{sensor = true} in playground.toml)
-- instead of hand-rolled distance-squared math against every PlayerControl
-- entity's Transform — engine.overlapping() reads rapier's own narrow-phase
-- intersection state.
--
-- ADR-0016: also the first real use of engine.play_sound, fired the same
-- tick as the despawn so a collect always has an audible confirmation
-- (games/sandbox/assets-src/pickup.ogg, CC0, via Kenney — see
-- pickup-CC0.txt).
local PICKUP_SOUND = "572963a19fd19711de1fa4eee4d5f503d7b28310d7dfe14b0201235c6c60507b"

function on_tick(components, tick, dt, self_id)
    if not engine.key_held("E") then
        return nil
    end

    local players = engine.query({ "PlayerControl" })
    local overlapping = engine.overlapping()
    for _, other_id in ipairs(overlapping) do
        for _, player in ipairs(players) do
            if other_id == player.id then
                engine.play_sound(PICKUP_SOUND, 1.0)
                engine.despawn()
                return nil
            end
        end
    end
    return nil
end
