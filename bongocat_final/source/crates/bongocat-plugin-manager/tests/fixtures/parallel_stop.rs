use std::io::{Read, Write};
use std::time::{Duration, Instant};

fn main() {
    let directory = std::env::current_dir().unwrap();
    let id = directory
        .parent()
        .unwrap()
        .file_name()
        .unwrap()
        .to_str()
        .unwrap();
    let store = directory.ancestors().nth(3).unwrap();
    let output = std::fs::read_to_string(directory.join("output.json")).unwrap();
    let mut input = std::io::stdin().lock();
    let mut stdout = std::io::stdout().lock();
    loop {
        let mut length = [0; 4];
        if input.read_exact(&mut length).is_err() {
            return;
        }
        let mut frame = vec![0; u32::from_le_bytes(length) as usize];
        input.read_exact(&mut frame).unwrap();
        let message = String::from_utf8(frame).unwrap();
        let sequence = message
            .split("\"sequence\":")
            .nth(1)
            .unwrap()
            .split(|c: char| !c.is_ascii_digit())
            .next()
            .unwrap();
        if message.contains("\"slow\"") {
            std::thread::sleep(Duration::from_secs(5));
        }
        let stopping = message.contains("\"stop\"");
        if stopping {
            std::fs::write(store.join(format!("stop-{id}")), b"ready").unwrap();
            let deadline = Instant::now() + Duration::from_secs(3);
            while !(store.join("stop-com.example.one").exists()
                && store.join("stop-com.example.two").exists())
            {
                if Instant::now() >= deadline {
                    std::process::exit(1);
                }
                std::thread::sleep(Duration::from_millis(5));
            }
        }
        let response = format!("{{\"sequence\":{sequence},\"result\":{{\"Ok\":{output}}}}}");
        stdout
            .write_all(&(response.len() as u32).to_le_bytes())
            .unwrap();
        stdout.write_all(response.as_bytes()).unwrap();
        stdout.flush().unwrap();
        if stopping {
            return;
        }
    }
}
