function on_tick(components, tick, dt)
    return { Counter = { value = components.Counter.value + 1 } }
end
