#[derive(Default, Debug, Copy, Clone)]
pub(crate) struct AssociationStats {
    n_datas: u64,
    n_sacks: u64,
    n_t3timeouts: u64,
    n_ack_timeouts: u64,
    n_fast_retrans: u64,
}

impl AssociationStats {
    pub(crate) fn inc_datas(&mut self) {
        self.n_datas += 1;
        //atomic.AddUint64(&s.n_datas, 1)
    }

    pub(crate) fn get_num_datas(&self) -> u64 {
        self.n_datas
        //return atomic.LoadUint64(&s.n_datas)
    }

    pub(crate) fn inc_sacks(&mut self) {
        self.n_sacks += 1;
        //atomic.AddUint64(&s.n_sacks, 1)
    }

    pub(crate) fn get_num_sacks(&self) -> u64 {
        self.n_sacks
        //return atomic.LoadUint64(&s.n_sacks)
    }

    pub(crate) fn inc_t3timeouts(&mut self) {
        self.n_t3timeouts += 1;
        //atomic.AddUint64(&s.n_t3timeouts, 1)
    }

    pub(crate) fn get_num_t3timeouts(&self) -> u64 {
        self.n_t3timeouts
        //return atomic.LoadUint64(&s.n_t3timeouts)
    }

    pub(crate) fn inc_ack_timeouts(&mut self) {
        self.n_ack_timeouts += 1;
        //atomic.AddUint64(&s.n_ack_timeouts, 1)
    }

    pub(crate) fn get_num_ack_timeouts(&self) -> u64 {
        self.n_ack_timeouts
        //return atomic.LoadUint64(&s.n_ack_timeouts)
    }

    pub(crate) fn inc_fast_retrans(&mut self) {
        self.n_fast_retrans += 1;
        //atomic.AddUint64(&s.n_fast_retrans, 1)
    }

    pub(crate) fn get_num_fast_retrans(&self) -> u64 {
        self.n_fast_retrans
        //return atomic.LoadUint64(&s.n_fast_retrans)
    }
}
