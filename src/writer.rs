// Copyright (C) 2025, Benjamin Drung <bdrung@posteo.de>
// SPDX-License-Identifier: ISC

fn sanitize_path<S: AsRef<str> + Into<String>>(path: S) -> String {
    match path.as_ref().strip_prefix("./") {
        Some(p) => {
            if p.is_empty() {
                ".".into()
            } else {
                p.into()
            }
        }
        None => path.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_path_dot() {
        assert_eq!(sanitize_path("."), ".");
    }

    #[test]
    fn test_sanitize_path_dot_slash() {
        assert_eq!(sanitize_path("./"), ".");
    }

    #[test]
    fn test_sanitize_path_dot_slash_path() {
        assert_eq!(sanitize_path("./path/to/file"), "path/to/file");
    }

    #[test]
    fn test_sanitize_path_relative_path() {
        assert_eq!(sanitize_path("path/to/file"), "path/to/file");
    }
}
