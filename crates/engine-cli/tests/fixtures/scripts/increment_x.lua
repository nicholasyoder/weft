function on_tick(components, tick, dt)
    return {
        Position = {
            x = components.Position.x + 1.0,
            y = components.Position.y,
            z = components.Position.z,
        },
    }
end
