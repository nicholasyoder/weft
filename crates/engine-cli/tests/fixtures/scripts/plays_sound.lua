-- ADR-0016 fixture: fires exactly one engine.play_sound at a known tick,
-- so `engine mix`'s golden-WAV test has an exact, reproducible moment to
-- check for the one-shot landing alongside the scene's looping AudioSource.
function on_tick(components, tick, dt, self_id)
    if tick == 5 then
        engine.play_sound("8177c945026317eac5e844ce58204a1e81037b48f8d709a5f3c4ab94cd01b917", 0.6)
    end
    return nil
end
