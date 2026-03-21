use rocket::form::FromForm;

#[derive(Debug, Clone, Copy, Default, FromForm)]
pub struct Pagination {
    pub limit: Option<u64>,
    pub skip: Option<u64>,
}

impl Pagination {
    pub const DEFAULT_LIMIT: u64 = 20;
    pub const MAX_LIMIT: u64 = 100;

    pub fn resolved_limit(self) -> u64 {
        self.limit
            .unwrap_or(Self::DEFAULT_LIMIT)
            .clamp(1, Self::MAX_LIMIT)
    }

    pub fn resolved_skip(self) -> u64 {
        self.skip.unwrap_or(0)
    }
}
