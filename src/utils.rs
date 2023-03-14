pub fn clear_or_update<T>(clear: bool, update: Option<T>) -> Option<Option<T>> {
    if clear {
        Some(None)
    } else {
        update.map(Some)
    }
}
