use super::cache_cleanup;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

fn project_roots(home: &Path, projects: usize, directories: usize, files: usize) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for project in 0..projects {
        let project = home.join("Work").join(format!("project-{project}"));
        for (artifact, marker) in [
            ("target", ".rustc_info.json"),
            ("node_modules", ".package-lock.json"),
            (".next", "build-manifest.json"),
            (".venv", "pyvenv.cfg"),
        ] {
            let path = project.join(artifact);
            std::fs::create_dir_all(&path).unwrap();
            std::fs::write(path.join(marker), b"{}").unwrap();
            for d in 0..directories {
                let dir = path.join(d.to_string());
                std::fs::create_dir(&dir).unwrap();
                for f in 0..files {
                    std::fs::write(dir.join(f.to_string()), [b'x'; 1024]).unwrap();
                }
            }
            paths.push(path);
        }
    }
    paths
}

fn reference_kib(path: &OsStr) -> u64 {
    let output = Command::new("/usr/bin/du")
        .arg("-skx")
        .arg(path)
        .output()
        .unwrap();
    assert!(output.status.success());
    let text = std::str::from_utf8(&output.stdout).unwrap();
    text.split_once('\t').unwrap().0.parse().unwrap()
}

#[test]
#[ignore = "local timing gate; uses only a synthetic tree and the macOS du oracle"]
fn cache_size_wall_time() {
    for (name, projects, directories, files) in
        [("many-roots", 32, 1, 12), ("large-roots", 1, 16, 128)]
    {
        let home = tempfile::tempdir().unwrap();
        let paths = project_roots(home.path(), projects, directories, files);
        let mut trials = [Vec::<Duration>::new(), Vec::new()];
        for round in 0..10 {
            let mut sizes = [Vec::new(), Vec::new()];
            for lane in [round % 2, 1 - round % 2] {
                let start = Instant::now();
                sizes[lane] = paths
                    .iter()
                    .map(|path| {
                        if lane == 0 {
                            reference_kib(path.as_os_str())
                        } else {
                            cache_cleanup::inspect(
                                home.path(),
                                path.strip_prefix(home.path()).unwrap(),
                                0,
                            )
                            .unwrap()
                            .unwrap()
                            .kib
                        }
                    })
                    .collect();
                let elapsed = start.elapsed();
                if round != 0 {
                    trials[lane].push(elapsed);
                }
            }
            assert_eq!(sizes[0], sizes[1], "{name}: per-target disk usage");
        }
        for (lane, values) in trials.iter_mut().enumerate() {
            values.sort();
            let label = ["du", "native"][lane];
            eprintln!(
                "{name} {label}: median={:?} range={:?}..{:?} n={}",
                values[4],
                values[0],
                values[8],
                values.len()
            );
        }
        if name == "many-roots" {
            assert!(
                trials[1][4] < trials[0][4],
                "one du subprocess per root must not be faster"
            );
        }
    }
}
