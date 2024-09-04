// Copyright (C) 2024, Benjamin Drung <bdrung@posteo.de>
// SPDX-License-Identifier: ISC

fn sanitize_path(path: &String) -> String {
    match path.strip_prefix("./") {
        Some(p) => {
            if p.len() == 0 {
                ".".into()
            } else {
                p.into()
            }
        }
        None => path.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_path_dot() {
        assert_eq!(sanitize_path(&".".into()), ".");
    }

    #[test]
    fn test_sanitize_path_dot_slash() {
        assert_eq!(sanitize_path(&"./".into()), ".");
    }

    #[test]
    fn test_sanitize_path_dot_slash_path() {
        assert_eq!(sanitize_path(&"./path/to/file".into()), "path/to/file");
    }

    #[test]
    fn test_sanitize_path_relative_path() {
        assert_eq!(sanitize_path(&"path/to/file".into()), "path/to/file");
    }
}
