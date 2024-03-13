use portable_atomic::AtomicU64;
use std::sync::atomic::Ordering;

#[derive(Default, Debug)]
pub(crate) struct AssociationStats {
    n_datas: AtomicU64,
    n_sacks: AtomicU64,
    n_t3timeouts: AtomicU64,
    n_ack_timeouts: AtomicU64,
    n_fast_retrans: AtomicU64,
}

impl AssociationStats {
    pub(crate) fn inc_datas(&self) {
        self.n_datas.fetch_add(1, Ordering::SeqCst);
    }

    pub(crate) fn get_num_datas(&self) -> u64 {
        self.n_datas.load(Ordering::SeqCst)
    }

    pub(crate) fn inc_sacks(&self) {
        self.n_sacks.fetch_add(1, Ordering::SeqCst);
    }

    pub(crate) fn get_num_sacks(&self) -> u64 {
        self.n_sacks.load(Ordering::SeqCst)
    }

    pub(crate) fn inc_t3timeouts(&self) {
        self.n_t3timeouts.fetch_add(1, Ordering::SeqCst);
    }

    pub(crate) fn get_num_t3timeouts(&self) -> u64 {
        self.n_t3timeouts.load(Ordering::SeqCst)
    }

    pub(crate) fn inc_ack_timeouts(&self) {
        self.n_ack_timeouts.fetch_add(1, Ordering::SeqCst);
    }

    pub(crate) fn get_num_ack_timeouts(&self) -> u64 {
        self.n_ack_timeouts.load(Ordering::SeqCst)
    }

    pub(crate) fn inc_fast_retrans(&self) {
        self.n_fast_retrans.fetch_add(1, Ordering::SeqCst);
    }

    pub(crate) fn get_num_fast_retrans(&self) -> u64 {
        self.n_fast_retrans.load(Ordering::SeqCst)
    }

    pub(crate) fn reset(&self) {
        self.n_datas.store(0, Ordering::SeqCst);
        self.n_sacks.store(0, Ordering::SeqCst);
        self.n_t3timeouts.store(0, Ordering::SeqCst);
        self.n_ack_timeouts.store(0, Ordering::SeqCst);
        self.n_fast_retrans.store(0, Ordering::SeqCst);
    }
}
