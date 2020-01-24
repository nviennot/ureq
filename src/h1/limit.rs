// use crate::peek::Peekable;
// use crate::AsyncReadExt;
// use crate::Stream;
// use std::io;

// pub struct LimitRead {
//     stream: Peekable<Box<dyn Stream>>,
//     left_to_read: usize,
// }

// impl LimitRead {
//     pub fn new(stream: Peekable<Box<dyn Stream>>, limit: usize) -> Self {
//         LimitRead {
//             stream,
//             left_to_read: limit,
//         }
//     }

//     pub fn into_inner(self) -> Peekable<Box<dyn Stream>> {
//         self.stream
//     }

//     pub async fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
//         let max = buf.len().max(self.left_to_read);
//         let amount = self.stream.read(&mut buf[..max]).await?;
//         self.left_to_read -= amount;
//         Ok(amount)
//     }
// }
