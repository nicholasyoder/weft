-- games/sandbox's second scripted mechanic, alongside pickup.lua — a lever
-- that opens the arena's gate when the player holds E within range *and*
-- with a real, unobstructed line of sight to it. Where pickup.lua's
-- interaction is proximity (engine.overlapping, a rapier sensor overlap),
-- this one is aim/line-of-sight (engine.raycast, physics-substrate-plan.md
-- Phase 3) — the first real, live use of engine.raycast in games/sandbox;
-- everywhere else it's only been exercised by engine-physics's own unit
-- tests. Casting from the lever toward the player (rather than from the
-- player's facing direction) sidesteps needing quaternion math in Lua:
-- PlayerControl's facing only updates while moving (see
-- games/sandbox/src/player_control.rs), so it isn't a reliable "aim"
-- vector on its own.
--
-- The gate itself is found by its Gate marker component (games/sandbox/src/
-- gate.rs), not a hardcoded entity id, and despawned by explicit id
-- (engine.despawn(id) — the same function pickup.lua calls with no id to
-- despawn itself). A hit reported against anything other than the querying
-- player's own id means an obstacle is in the way, so the check is a
-- straightforward id comparison, not a distance/thing tolerance.
local GATE_OPEN_SOUND = "572963a19fd19711de1fa4eee4d5f503d7b28310d7dfe14b0201235c6c60507b"
local MAX_DISTANCE = 3.5

local function length(v)
    return math.sqrt(v[1] * v[1] + v[2] * v[2] + v[3] * v[3])
end

local function normalize(v)
    local len = length(v)
    return { v[1] / len, v[2] / len, v[3] / len }
end

function on_tick(components, tick, dt, self_id)
    if not engine.key_held("E") then
        return nil
    end

    local origin = components.Transform.position
    local players = engine.query({ "PlayerControl", "Transform" })
    for _, player in ipairs(players) do
        local to_player = {
            player.Transform.position[1] - origin[1],
            player.Transform.position[2] - origin[2],
            player.Transform.position[3] - origin[3],
        }
        local distance = length(to_player)
        if distance <= MAX_DISTANCE then
            local hit = engine.raycast(origin, normalize(to_player), MAX_DISTANCE)
            if hit ~= nil and hit.id == player.id then
                local gates = engine.query({ "Gate" })
                for _, gate in ipairs(gates) do
                    engine.despawn(gate.id)
                end
                engine.play_sound(GATE_OPEN_SOUND, 1.0)
                engine.despawn()
                return nil
            end
        end
    end
    return nil
end
