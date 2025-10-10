use rtcp::transport_feedbacks::transport_layer_nack::NackPair;
use rtp::packet::Packet;
use std::time::{Duration, Instant};
use util::sync::RwLock;

#[derive(Clone, Debug)]
pub struct PCache {
    // Финальный пакет, уже с переписанными SSRC/SN/TS и всеми нужными расширениями
    // Можно добавить счётчик/время ретрансляций для отладочной метрики (AtomicU32/Option<Instant>)
    pub packet: Packet,         // полный RTP (ingress)
    pub first_sent_at: Instant, // для TTL
}

#[derive(Debug)]
pub struct PCacheBuffer {
    ttl: Duration,
    capacity: usize, // степень двойки предпочтительно; capacity - 1, если capacity — power-of-two
    slots: RwLock<Vec<Option<PCache>>>,
}

impl PCacheBuffer {
    pub fn new(ttl: Duration, capacity_pow2: usize) -> Self {
        assert!(capacity_pow2.is_power_of_two());
        Self {
            ttl,
            slots: RwLock::new(vec![None; capacity_pow2]),
            capacity: capacity_pow2 - 1,
        }
    }

    #[inline]
    fn idx(&self, seq: u16) -> usize {
        (seq as usize) & self.capacity
    }

    pub fn put(&self, packet: Packet) {
        let idx = self.idx(packet.header.sequence_number);
        let mut slots = self.slots.write();
        slots[idx] = Some(PCache {
            packet,
            first_sent_at: Instant::now(),
        });
    }

    pub fn get(&self, seq: u16) -> Option<Packet> {
        let idx = self.idx(seq);
        let slots = self.slots.read();
        let some = slots.get(idx)?.as_ref()?;
        if some.packet.header.sequence_number != seq {
            println!(
                "Коллизия кольца: запрошен seq={seq}. В кеше seq={}",
                some.packet.header.sequence_number
            );
            return None; // коллизия кольца (wrap)
        }
        let elapsed = some.first_sent_at.elapsed();
        if elapsed > self.ttl {
            println!(
                "Пакет просрочен. Прошло {:?}, что больше ttl = {:?}",
                elapsed, self.ttl
            );
            return None; // просрочен
        }
        Some(some.packet.clone())
    }
}

// Вспомогательная функция разворачивания NACK-пар (packet_id + bitmask -> список seq)
// Разворачиваем список потерянных SN из NACK-пар
pub fn expand_nack_pairs(pairs: &[NackPair]) -> Vec<u16> {
    let mut out = Vec::with_capacity(pairs.len() * 8);
    for p in pairs {
        let base = p.packet_id;
        out.push(base);
        let mut mask = p.lost_packets;
        let mut i = 0;
        while mask != 0 {
            if (mask & 1) != 0 {
                out.push(base.wrapping_add(i + 1));
            }
            mask >>= 1;
            i += 1;
        }
    }
    out
}
