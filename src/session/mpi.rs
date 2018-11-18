use crate::session::SessionMan;

pub struct SessionMPI<'a> {
    mgr: &'a SessionMan,
    nproc: u32,
}

impl<'a> SessionMPI<'a> {
    pub fn new(mgr: &'a SessionMan, nproc: u32) -> Self {
        SessionMPI { mgr, nproc }
    }
}
