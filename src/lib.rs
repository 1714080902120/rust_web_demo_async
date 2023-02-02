use std::{
    sync::{
        mpsc::{self, Receiver},
        Arc, Mutex,
    },
    thread,
};

use std::{fs, str::from_utf8, time::Duration};

use async_std::{
    self,
    prelude::*,
    task::{self},
    io::{ Read, Write }
};

pub struct ThreadPool {
    workers: Vec<Worker>,
    sender: Option<mpsc::Sender<Job>>,
}

impl ThreadPool {
    pub fn new(size: usize) -> ThreadPool {
        assert!(size > 0);

        let (sender, receiver) = mpsc::channel();

        let receiver = Arc::new(Mutex::new(receiver));

        let mut workers = Vec::with_capacity(size);

        for id in 0..size {
            workers.push(Worker::new(id, Arc::clone(&receiver)));
        }

        ThreadPool {
            workers,
            sender: Some(sender),
        }
    }

    pub fn execute<F>(&self, f: F)
    where
        F: FnOnce() + Send + 'static,
    {
        let job = Box::new(f);

        self.sender.as_ref().unwrap().send(job).unwrap();
    }
}

impl Drop for ThreadPool {
    fn drop(&mut self) {
        drop(self.sender.take());

        for worker in &mut self.workers {
            if let Some(thread) = worker.thread.take() {
                thread.join().unwrap();
            }
        }
    }
}

struct Worker {
    id: usize,
    thread: Option<thread::JoinHandle<()>>,
}

impl Worker {
    fn new(id: usize, receiver: Arc<Mutex<Receiver<Job>>>) -> Worker {
        let thread = thread::spawn(move || loop {
            let job = receiver.lock().unwrap().recv();

            match job {
                Ok(v) => {
                    v();
                }
                Err(_) => {
                    println!("thread {id} is close");
                    break;
                }
            }

            println!("Worker {id} got a job; executing.");
        });
        Worker {
            id,
            thread: Some(thread),
        }
    }
}

type Job = Box<dyn FnOnce() + Send + 'static>;


pub async fn handle_connection(mut stream: impl Read + Write + Unpin) {
    let mut buffer = [0; 1024];
    stream.read(&mut buffer).await.unwrap();
    let req: Vec<&str> = from_utf8(&buffer).unwrap().split("\r\n").filter(|item| { !item.is_empty() }).collect();
    let str_list: Vec<&str> = req.get(0).unwrap().split_ascii_whitespace().collect();
    let path = str_list.get(1).unwrap();
    let (status_line, file_path) = match *path {
        "/" => ("HTTP/1.1 200 OK", "index.html"),
        "/sleep" => {
            task::sleep(Duration::from_secs(5)).await;
            ("HTTP/1.1 200 OK", "index.html")
        }
        _ => ("HTTP/1.1 404 NOT FOUND", "404.html"),
    };

    let res_body = fs::read_to_string(file_path).unwrap();
    let res_headers = format!("Content-Type: html;\r\nContent-Length:{}", res_body.len());
    let res = format!("{status_line}\r\n{res_headers}\r\n\r\n{res_body}");
    stream.write(&res.as_bytes()).await.unwrap();
    stream.flush().await.unwrap();
}


#[cfg(test)]

mod tests {
    use super::{handle_connection};

    use async_std::io::{ Read, Write };
    use futures::io::Error;
    use futures::task::{Context, Poll};

    use std::cmp::min;
    use std::pin::Pin;

    struct MockData {
        read_data: Vec<u8>,
        write_data: Vec<u8>,
    }

    impl Read for MockData {
        fn poll_read(
            self: Pin<&mut Self>,
            _: &mut Context,
            buf: &mut [u8],
        ) -> Poll<Result<usize, Error>> {
            let size: usize = min(self.read_data.len(), buf.len());
            buf[..size].copy_from_slice(&self.read_data[..size]);
            Poll::Ready(Ok(size))
        }
    }

    impl Write for MockData {
        fn poll_write(
            mut self: Pin<&mut Self>,
            _: &mut Context,
            buf: &[u8],
        ) -> Poll<Result<usize, Error>> {
            self.write_data = Vec::from(buf);

            Poll::Ready(Ok(buf.len()))
        }

        fn poll_flush(self: Pin<&mut Self>, _: &mut Context) -> Poll<Result<(), Error>> {
            Poll::Ready(Ok(()))
        }

        fn poll_close(self: Pin<&mut Self>, _: &mut Context) -> Poll<Result<(), Error>> {
            Poll::Ready(Ok(()))
        }
    }

    impl Unpin for MockData {}

    use std::fs;
    #[async_std::test]
    async fn test_handle_connection() {
        let input_bytes = b"GET / HTTP/1.1\r\n";
        let mut contents = vec![0u8; 1024];
        contents[..input_bytes.len()].clone_from_slice(input_bytes);
        let mut stream = MockData {
            read_data: contents,
            write_data: Vec::new(),
        };

        handle_connection(&mut stream).await;

        let expected_contents = fs::read_to_string("index.html").unwrap();
        let expected_headers = format!("Content-Type: html;\r\nContent-Length:{}", expected_contents.len());
        let expected_response = format!("HTTP/1.1 200 OK\r\n{}\r\n\r\n{}", expected_headers,expected_contents);
        assert!(stream.write_data.starts_with(expected_response.as_bytes()));
    }
}
