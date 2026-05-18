use serde::{Deserialize, Serialize};

/// Standard pagination input — mirrors the TS `PaginationInput` Zod schema.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PageInput {
    pub page: u64,
    pub page_size: u64,
}

impl Default for PageInput {
    fn default() -> Self {
        Self { page: 1, page_size: 20 }
    }
}

impl PageInput {
    /// Returns the SQL OFFSET value for this page.
    pub fn offset(&self) -> u64 {
        (self.page.saturating_sub(1)) * self.page_size
    }

    pub fn limit(&self) -> u64 {
        self.page_size
    }
}

/// Standard paginated response — mirrors the TS `PaginatedOutput` Zod schema.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Page<T: Serialize> {
    pub items: Vec<T>,
    pub total: i64,
    pub page: u64,
    pub page_size: u64,
    pub total_pages: u64,
}

impl<T: Serialize> Page<T> {
    pub fn new(items: Vec<T>, total: i64, input: &PageInput) -> Self {
        let total_pages = if input.page_size == 0 {
            0
        } else {
            total.unsigned_abs().div_ceil(input.page_size)
        };
        Self {
            items,
            total,
            page: input.page,
            page_size: input.page_size,
            total_pages,
        }
    }

    /// Convenience constructor for repos that return `(Vec<T>, i64)` with raw `page/page_size` values.
    pub fn from_parts(items: Vec<T>, total: i64, page: u64, page_size: u64) -> Self {
        Self::new(items, total, &PageInput { page, page_size })
    }
}
