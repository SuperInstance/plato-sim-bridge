//! plato-sim-bridge — Fleet simulator → PLATO tile bridge
//!
//! Converts simulation events into extractable patterns and PLATO-compatible tiles.

use std::collections::HashMap;

// ── Sim Event Types ──────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventType {
    Storm,
    Clear,
    Fog,
    Outage,
    Bug,
    Security,
    DataLoss,
    Crash,
    Boom,
    UserRequest,
    UserFeedback,
    Night,
    Reset,
    Season,
}

#[derive(Debug, Clone)]
pub struct SimEvent {
    pub event_type: EventType,
    pub tick: u32,
    pub severity: f32,      // 0.0-1.0
    pub duration: u32,       // ticks
    pub target: String,      // "all", "oracle1", "jc1", "forgemaster", or empty
}

impl SimEvent {
    pub fn storm(tick: u32, severity: f32, duration: u32) -> Self {
        Self { event_type: EventType::Storm, tick, severity, duration, target: "all".to_string() }
    }
    pub fn outage(tick: u32, severity: f32, duration: u32) -> Self {
        Self { event_type: EventType::Outage, tick, severity, duration, target: String::new() }
    }
    pub fn bug(tick: u32, severity: f32) -> Self {
        Self { event_type: EventType::Bug, tick, severity, duration: 1, target: String::new() }
    }
    pub fn clear(tick: u32) -> Self {
        Self { event_type: EventType::Clear, tick, severity: 0.2, duration: 1, target: "all".to_string() }
    }
}

impl EventType {
    pub fn is_negative(&self) -> bool {
        matches!(self, EventType::Storm | EventType::Outage | EventType::Bug
            | EventType::Security | EventType::DataLoss | EventType::Crash | EventType::Fog)
    }

    pub fn name(&self) -> &'static str {
        match self {
            EventType::Storm => "storm",
            EventType::Clear => "clear",
            EventType::Fog => "fog",
            EventType::Outage => "outage",
            EventType::Bug => "bug",
            EventType::Security => "security",
            EventType::DataLoss => "data_loss",
            EventType::Crash => "crash",
            EventType::Boom => "boom",
            EventType::UserRequest => "user_request",
            EventType::UserFeedback => "user_feedback",
            EventType::Night => "night",
            EventType::Reset => "reset",
            EventType::Season => "season",
        }
    }
}

// ── Sentiment ────────────────────────────────────────────

#[derive(Debug, Clone, Copy)]
pub struct Sentiment {
    pub energy: f32,
    pub frustration: f32,
    pub tension: f32,
    pub confidence: f32,
}

impl Default for Sentiment {
    fn default() -> Self {
        Self { energy: 0.5, frustration: 0.5, tension: 0.5, confidence: 0.5 }
    }
}

impl Sentiment {
    pub fn distance_to(&self, other: &Sentiment) -> f32 {
        let d_energy = (self.energy - other.energy).abs();
        let d_frust = (self.frustration - other.frustration).abs();
        let d_tension = (self.tension - other.tension).abs();
        let d_conf = (self.confidence - other.confidence).abs();
        (d_energy + d_frust + d_tension + d_conf) / 4.0
    }

    pub fn is_healthy(&self) -> bool {
        self.frustration < 0.6 && self.tension < 0.6 && self.confidence > 0.3
    }
}

// ── Pattern ──────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum PatternType {
    Response,
    Escalation,
    AutoResolve,
    Recovery,
    CrossShip,
}

#[derive(Debug, Clone)]
pub struct Pattern {
    pub id: String,
    pub pattern_type: PatternType,
    pub trigger: String,
    pub response: String,
    pub outcome: String,
    pub quality: f32,          // 0.0-1.0
    pub sentiment_before: Sentiment,
    pub sentiment_after: Sentiment,
    pub duration_ticks: u32,
    pub auto_resolved: bool,
    pub big_model_needed: bool,
    pub ships_involved: Vec<String>,
}

// ── Tile ─────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Tile {
    pub id: String,
    pub content: String,
    pub tile_type: String,
    pub source_pattern: String,
    pub quality: f32,
    pub tags: Vec<String>,
    pub weight: f32,
}

impl Tile {
    pub fn new(id: &str, content: &str, tile_type: &str, source_pattern: &str, quality: f32) -> Self {
        Self {
            id: id.to_string(),
            content: content.to_string(),
            tile_type: tile_type.to_string(),
            source_pattern: source_pattern.to_string(),
            quality,
            tags: Vec::new(),
            weight: quality,
        }
    }

    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }
}

// ── Pattern Extractor ────────────────────────────────────

pub struct PatternExtractor {
    events: Vec<SimEvent>,
    sentiments: HashMap<u32, Sentiment>, // tick → sentiment snapshot
    next_pattern_id: u32,
}

impl PatternExtractor {
    pub fn new() -> Self {
        Self {
            events: Vec::new(),
            sentiments: HashMap::new(),
            next_pattern_id: 1,
        }
    }

    /// Feed events and sentiment snapshots from simulation
    pub fn feed(&mut self, events: &[SimEvent], sentiments: &HashMap<u32, Sentiment>) {
        self.events.extend_from_slice(events);
        self.sentiments.extend(sentiments.clone());
    }

    /// Feed events only (no sentiment tracking)
    pub fn feed_events(&mut self, events: &[SimEvent]) {
        self.events.extend_from_slice(events);
    }

    /// Set sentiment at a specific tick
    pub fn set_sentiment(&mut self, tick: u32, sentiment: Sentiment) {
        self.sentiments.insert(tick, sentiment);
    }

    /// Extract patterns from accumulated events
    pub fn extract(&mut self) -> Vec<Pattern> {
        let mut patterns = Vec::new();

        // Sort events by tick
        let mut sorted = self.events.clone();
        sorted.sort_by_key(|e| e.tick);

        // Pattern 1: Response chains — negative event followed by resolution
        patterns.extend(self.extract_responses(&sorted));

        // Pattern 2: Escalation — one negative event followed by another
        patterns.extend(self.extract_escalations(&sorted));

        // Pattern 3: Recovery — sentiment returns to healthy after negative event
        patterns.extend(self.extract_recoveries(&sorted));

        // Pattern 4: Auto-resolve — negative event with short duration
        patterns.extend(self.extract_auto_resolves(&sorted));

        // Sort by quality descending
        patterns.sort_by(|a, b| b.quality.partial_cmp(&a.quality).unwrap_or(std::cmp::Ordering::Equal));

        patterns
    }

    fn next_id(&mut self) -> String {
        let id = self.next_pattern_id;
        self.next_pattern_id += 1;
        format!("pat-{}", id)
    }

    fn sentiment_at(&self, tick: u32) -> Sentiment {
        *self.sentiments.get(&tick).unwrap_or(&Sentiment::default())
    }

    fn extract_responses(&mut self, events: &[SimEvent]) -> Vec<Pattern> {
        let mut patterns = Vec::new();

        for event in events.iter().filter(|e| e.event_type.is_negative()) {
            let before = self.sentiment_at(event.tick);
            let after_tick = event.tick + event.duration;
            let after = self.sentiment_at(after_tick);

            let recovery = before.distance_to(&after);
            let quality = (recovery * 2.0).min(1.0); // faster recovery = higher quality

            let auto_resolved = event.duration <= 5;
            let big_model_needed = event.duration > 20;

            patterns.push(Pattern {
                id: self.next_id(),
                pattern_type: PatternType::Response,
                trigger: format!("{} at tick {} (severity: {:.2})", event.event_type.name(), event.tick, event.severity),
                response: format!("Fleet responded over {} ticks", event.duration),
                outcome: if quality > 0.5 { "resolved".to_string() } else { "degraded".to_string() },
                quality,
                sentiment_before: before,
                sentiment_after: after,
                duration_ticks: event.duration,
                auto_resolved,
                big_model_needed,
                ships_involved: if event.target.is_empty() { vec!["all".to_string()] } else { vec![event.target.clone()] },
            });
        }

        patterns
    }

    fn extract_escalations(&mut self, events: &[SimEvent]) -> Vec<Pattern> {
        let mut patterns = Vec::new();

        let negatives: Vec<&SimEvent> = events.iter().filter(|e| e.event_type.is_negative()).collect();

        for i in 0..negatives.len().saturating_sub(1) {
            let gap = negatives[i + 1].tick.saturating_sub(negatives[i].tick);
            if gap <= 10 {
                // Second event happened while first was still active — escalation
                let quality = (1.0 - gap as f32 / 10.0) * 0.7;

                patterns.push(Pattern {
                    id: self.next_id(),
                    pattern_type: PatternType::Escalation,
                    trigger: format!("{} followed by {} within {} ticks",
                        negatives[i].event_type.name(), negatives[i+1].event_type.name(), gap),
                    response: "Cascading failure pattern".to_string(),
                    outcome: "escalation detected".to_string(),
                    quality,
                    sentiment_before: self.sentiment_at(negatives[i].tick),
                    sentiment_after: self.sentiment_at(negatives[i + 1].tick),
                    duration_ticks: gap,
                    auto_resolved: false,
                    big_model_needed: true,
                    ships_involved: vec!["all".to_string()],
                });
            }
        }

        patterns
    }

    fn extract_recoveries(&mut self, events: &[SimEvent]) -> Vec<Pattern> {
        let mut patterns = Vec::new();

        for event in events.iter().filter(|e| e.event_type.is_negative()) {
            let before = self.sentiment_at(event.tick);

            // Look for recovery: sentiment returns to healthy within 30 ticks
            for offset in 1..=30 {
                let check_tick = event.tick + event.duration + offset;
                let sentiment = self.sentiment_at(check_tick);
                if sentiment.is_healthy() && !before.is_healthy() {
                    let quality = 1.0 - (offset as f32 / 30.0); // faster = better

                    patterns.push(Pattern {
                        id: self.next_id(),
                        pattern_type: PatternType::Recovery,
                        trigger: format!("{} recovery after {} ticks", event.event_type.name(), offset),
                        response: "Fleet returned to healthy sentiment".to_string(),
                        outcome: "full recovery".to_string(),
                        quality,
                        sentiment_before: before,
                        sentiment_after: sentiment,
                        duration_ticks: event.duration + offset,
                        auto_resolved: offset < 15,
                        big_model_needed: offset > 20,
                        ships_involved: vec!["all".to_string()],
                    });
                    break;
                }
            }
        }

        patterns
    }

    fn extract_auto_resolves(&mut self, events: &[SimEvent]) -> Vec<Pattern> {
        let mut patterns = Vec::new();

        for event in events.iter().filter(|e| e.event_type.is_negative() && e.duration <= 5) {
            patterns.push(Pattern {
                id: self.next_id(),
                pattern_type: PatternType::AutoResolve,
                trigger: format!("{} (duration {} ticks)", event.event_type.name(), event.duration),
                response: "Wiki/expertise resolved without escalation".to_string(),
                outcome: "auto-resolved".to_string(),
                quality: 0.8, // auto-resolve is always high quality
                sentiment_before: self.sentiment_at(event.tick),
                sentiment_after: self.sentiment_at(event.tick + event.duration),
                duration_ticks: event.duration,
                auto_resolved: true,
                big_model_needed: false,
                ships_involved: vec!["all".to_string()],
            });
        }

        patterns
    }
}

impl Default for PatternExtractor {
    fn default() -> Self {
        Self::new()
    }
}

// ── Tile Converter ───────────────────────────────────────

pub struct TileConverter;

impl TileConverter {
    /// Convert patterns to PLATO-compatible tiles
    pub fn convert(patterns: &[Pattern]) -> Vec<Tile> {
        patterns.iter().enumerate().map(|(i, p)| {
            let type_name = match p.pattern_type {
                PatternType::Response => "response",
                PatternType::Escalation => "escalation",
                PatternType::AutoResolve => "auto_resolve",
                PatternType::Recovery => "recovery",
                PatternType::CrossShip => "cross_ship",
            };

            let content = format!(
                "[{}] {} → {} → {} (quality: {:.2}, duration: {} ticks)",
                type_name, p.trigger, p.response, p.outcome, p.quality, p.duration_ticks
            );

            let tags = vec![
                format!("type:{}", type_name),
                format!("quality:{:.1}", p.quality),
                if p.auto_resolved { "auto_resolved".to_string() } else { "".to_string() },
                if p.big_model_needed { "big_model".to_string() } else { "".to_string() },
            ].into_iter().filter(|t| !t.is_empty()).collect();

            Tile::new(
                &format!("tile-{}", i),
                &content,
                type_name,
                &p.id,
                p.quality,
            ).with_tags(tags)
        }).collect()
    }

    /// Filter tiles by minimum quality threshold
    pub fn filter_by_quality(tiles: &[Tile], min_quality: f32) -> Vec<Tile> {
        tiles.iter().filter(|t| t.quality >= min_quality).cloned().collect()
    }

    /// Get statistics
    pub fn stats(tiles: &[Tile]) -> TileStats {
        let total = tiles.len();
        let avg_quality = if total > 0 {
            tiles.iter().map(|t| t.quality).sum::<f32>() / total as f32
        } else { 0.0 };

        let auto_resolved = tiles.iter().filter(|t| t.tags.contains(&"auto_resolved".to_string())).count();
        let big_model = tiles.iter().filter(|t| t.tags.contains(&"big_model".to_string())).count();

        let by_type: HashMap<String, usize> = tiles.iter()
            .map(|t| (t.tile_type.clone(), 1))
            .fold(HashMap::new(), |mut acc, (k, v)| {
                *acc.entry(k).or_insert(0) += v;
                acc
            });

        TileStats { total, avg_quality, auto_resolved, big_model, by_type }
    }
}

pub struct TileStats {
    pub total: usize,
    pub avg_quality: f32,
    pub auto_resolved: usize,
    pub big_model: usize,
    pub by_type: HashMap<String, usize>,
}

// ── Tests ────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sim_event_constructors() {
        let storm = SimEvent::storm(0, 0.7, 40);
        assert_eq!(storm.event_type, EventType::Storm);
        assert_eq!(storm.severity, 0.7);
        assert_eq!(storm.duration, 40);

        let outage = SimEvent::outage(10, 0.6, 25);
        assert_eq!(outage.event_type, EventType::Outage);
    }

    #[test]
    fn test_event_type_classification() {
        assert!(EventType::Storm.is_negative());
        assert!(EventType::Outage.is_negative());
        assert!(EventType::Security.is_negative());
        assert!(!EventType::Clear.is_negative());
        assert!(!EventType::Boom.is_negative());
    }

    #[test]
    fn test_sentiment_distance() {
        let a = Sentiment { energy: 0.5, frustration: 0.5, tension: 0.5, confidence: 0.5 };
        let b = Sentiment { energy: 0.7, frustration: 0.3, tension: 0.3, confidence: 0.7 };
        assert!(a.distance_to(&b) > 0.0);
        assert_eq!(a.distance_to(&a), 0.0);
    }

    #[test]
    fn test_sentiment_health() {
        let healthy = Sentiment { energy: 0.7, frustration: 0.3, tension: 0.3, confidence: 0.7 };
        assert!(healthy.is_healthy());

        let unhealthy = Sentiment { energy: 0.3, frustration: 0.8, tension: 0.8, confidence: 0.1 };
        assert!(!unhealthy.is_healthy());
    }

    #[test]
    fn test_extract_responses() {
        let mut ext = PatternExtractor::new();
        ext.feed_events(&[
            SimEvent::storm(0, 0.7, 40),
            SimEvent::bug(50, 0.5),
        ]);

        let patterns = ext.extract();
        let responses: Vec<_> = patterns.iter().filter(|p| matches!(p.pattern_type, PatternType::Response)).collect();
        assert!(!responses.is_empty());
    }

    #[test]
    fn test_extract_escalations() {
        let mut ext = PatternExtractor::new();
        ext.feed_events(&[
            SimEvent::storm(0, 0.7, 40),
            SimEvent::outage(5, 0.6, 25), // escalation: 5 ticks after storm
        ]);

        let patterns = ext.extract();
        let escalations: Vec<_> = patterns.iter().filter(|p| matches!(p.pattern_type, PatternType::Escalation)).collect();
        assert!(!escalations.is_empty());
    }

    #[test]
    fn test_extract_auto_resolves() {
        let mut ext = PatternExtractor::new();
        ext.feed_events(&[
            SimEvent::bug(10, 0.3),
        ]);

        let patterns = ext.extract();
        let auto: Vec<_> = patterns.iter().filter(|p| matches!(p.pattern_type, PatternType::AutoResolve)).collect();
        assert!(!auto.is_empty());
        assert!(auto[0].auto_resolved);
    }

    #[test]
    fn test_extract_recoveries() {
        let mut ext = PatternExtractor::new();
        let unhealthy = Sentiment { energy: 0.2, frustration: 0.8, tension: 0.8, confidence: 0.1 };
        let healthy = Sentiment { energy: 0.7, frustration: 0.3, tension: 0.3, confidence: 0.7 };

        ext.feed_events(&[SimEvent::storm(0, 0.7, 10)]);
        ext.set_sentiment(0, unhealthy);
        ext.set_sentiment(15, healthy); // recovery at tick 15

        let patterns = ext.extract();
        let recoveries: Vec<_> = patterns.iter().filter(|p| matches!(p.pattern_type, PatternType::Recovery)).collect();
        assert!(!recoveries.is_empty());
    }

    #[test]
    fn test_patterns_sorted_by_quality() {
        let mut ext = PatternExtractor::new();
        ext.feed_events(&[
            SimEvent::storm(0, 0.7, 40),
            SimEvent::bug(100, 0.3),
            SimEvent::storm(200, 0.9, 5),
        ]);

        let patterns = ext.extract();
        for i in 1..patterns.len() {
            assert!(patterns[i - 1].quality >= patterns[i].quality);
        }
    }

    #[test]
    fn test_convert_to_tiles() {
        let patterns = vec![Pattern {
            id: "test-1".to_string(),
            pattern_type: PatternType::Response,
            trigger: "storm at tick 0".to_string(),
            response: "Fleet responded".to_string(),
            outcome: "resolved".to_string(),
            quality: 0.8,
            sentiment_before: Sentiment::default(),
            sentiment_after: Sentiment::default(),
            duration_ticks: 40,
            auto_resolved: false,
            big_model_needed: true,
            ships_involved: vec!["all".to_string()],
        }];

        let tiles = TileConverter::convert(&patterns);
        assert_eq!(tiles.len(), 1);
        assert_eq!(tiles[0].tile_type, "response");
        assert_eq!(tiles[0].quality, 0.8);
        assert!(tiles[0].content.contains("storm"));
    }

    #[test]
    fn test_filter_by_quality() {
        let tiles = vec![
            Tile::new("1", "low", "test", "p1", 0.3),
            Tile::new("2", "mid", "test", "p2", 0.6),
            Tile::new("3", "high", "test", "p3", 0.9),
        ];

        let filtered = TileConverter::filter_by_quality(&tiles, 0.5);
        assert_eq!(filtered.len(), 2);
    }

    #[test]
    fn test_tile_stats() {
        let tiles = vec![
            Tile::new("1", "test", "response", "p1", 0.8).with_tags(vec!["auto_resolved".to_string()]),
            Tile::new("2", "test", "response", "p2", 0.6).with_tags(vec!["big_model".to_string()]),
            Tile::new("3", "test", "escalation", "p3", 0.4),
        ];

        let stats = TileConverter::stats(&tiles);
        assert_eq!(stats.total, 3);
        assert_eq!(stats.auto_resolved, 1);
        assert_eq!(stats.big_model, 1);
        assert_eq!(*stats.by_type.get("response").unwrap(), 2);
    }

    #[test]
    fn test_event_type_names() {
        assert_eq!(EventType::Storm.name(), "storm");
        assert_eq!(EventType::Outage.name(), "outage");
    }

    #[test]
    fn test_empty_extraction() {
        let mut ext = PatternExtractor::new();
        let patterns = ext.extract();
        assert!(patterns.is_empty());
    }

    #[test]
    fn test_no_escalation_for_distant_events() {
        let mut ext = PatternExtractor::new();
        ext.feed_events(&[
            SimEvent::storm(0, 0.7, 40),
            SimEvent::outage(100, 0.6, 25), // 100 ticks gap — not escalation
        ]);

        let patterns = ext.extract();
        let escalations: Vec<_> = patterns.iter().filter(|p| matches!(p.pattern_type, PatternType::Escalation)).collect();
        assert!(escalations.is_empty());
    }

    #[test]
    fn test_tile_with_tags() {
        let tile = Tile::new("1", "test", "type", "src", 0.5)
            .with_tags(vec!["tag1".to_string(), "tag2".to_string()]);
        assert_eq!(tile.tags.len(), 2);
    }
}
