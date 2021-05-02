const PADDING_MULTIPLE: usize = 4;

pub(crate) fn get_padding_size(len: usize) -> usize {
    (PADDING_MULTIPLE - (len % PADDING_MULTIPLE)) % PADDING_MULTIPLE
}
