-- ADR-0013 CLI-level proof: exercises engine.key_held through the full
-- batch (run_and_dump) path, which always dispatches with a fixed, empty
-- Input::default() (see step_and_dispatch in engine-cli/src/lib.rs) — so
-- every key reads as not-held here regardless of what a real player does.
-- Confirms the binding works end-to-end with no panics/errors, not that
-- live keys are observed (that's engine-script's own dispatch-level test).
function on_tick(components, tick, dt)
    local x = engine.key_held("W") and 1.0 or 0.0
    return { Position = { x = x, y = 0.0, z = 0.0 } }
end
