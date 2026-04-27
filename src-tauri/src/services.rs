use crate::models::AppPaths;

#[derive(Clone, Debug)]
pub struct AppServices {
    pub paths: AppPaths,
}

impl Default for AppServices {
    fn default() -> Self {
        Self {
            paths: crate::config::default_paths(),
        }
    }
}
