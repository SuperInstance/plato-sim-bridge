# plato-sim-bridge

Fleet simulator → PLATO tile bridge. Converts simulation patterns into training tiles for real PLATO rooms.

## What This Does

Runs fleet simulations (storms, seasons, drills), extracts high-quality patterns, and converts them to PLATO-compatible tiles. The output feeds into plato-torch rooms for training and plato-ensign for distillation.

## The Loop

```
Simulate → Extract Patterns → Convert to Tiles → Train Rooms → Export Ensigns → Simulate Better
```

## Quick Start

```rust
use plato_sim_bridge::{SimEvent, PatternExtractor, TileConverter};

let mut extractor = PatternExtractor::new();
let events = vec![
    SimEvent::storm(0, 0.7, 40),
    SimEvent::outage(10, 0.6, 25),
];

extractor.feed(&events);
let patterns = extractor.extract();
let tiles = TileConverter::convert(&patterns);
// Feed tiles to plato-torch rooms
```

## Pattern Types

| Type | Description |
|------|-------------|
| `response` | Event → fleet action → resolution chain |
| `escalation` | One problem triggering another |
| `auto_resolve` | Wiki/expertise solved it without expensive model |
| `recovery` | Fleet bouncing back from adversity |
| `cross_ship` | One ship helping another |

## Quality Scoring

Patterns scored 0.0-1.0 based on:
- **Speed**: Faster resolution = higher quality
- **Auto-resolve**: Wiki solved it (no expensive model call needed)
- **Recovery**: Fleet recovered to baseline sentiment
- **Cross-ship**: Cooperation involved

## Integration

- `fleet-simulator`: Source simulation data (SuperInstance/fleet-simulator)
- `plato-tile-spec`: Output tile format
- `plato-torch`: Training room input
- `plato-ensign`: Distillation target

## License

MIT
