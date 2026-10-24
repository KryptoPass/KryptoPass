use crate::{SearchError, allocation_error};
use kryptopass_secure_memory::ProtectedBytes;
use zeroize::Zeroize;

#[derive(Clone, Copy)]
pub(crate) struct Span {
    block: usize,
    offset: usize,
    length: usize,
    generation: u64,
}

struct Hole {
    offset: usize,
    length: usize,
}

struct Block {
    bytes: ProtectedBytes,
    holes: Vec<Hole>,
    live: usize,
    generation: u64,
}

pub(crate) struct Arena {
    blocks: Vec<Option<Block>>,
    maximum: usize,
    block_size: usize,
    generation: u64,
}

impl Arena {
    pub fn new(maximum: usize, block_size: usize) -> Self {
        Self {
            blocks: Vec::new(),
            maximum,
            block_size,
            generation: 0,
        }
    }

    pub fn insert(&mut self, bytes: &[u8]) -> Result<Span, SearchError> {
        if bytes.is_empty() {
            return Ok(Span {
                block: usize::MAX,
                offset: 0,
                length: 0,
                generation: 0,
            });
        }
        for (index, block) in self.blocks.iter_mut().enumerate() {
            let Some(block) = block else {
                continue;
            };
            if let Some(hole) = block
                .holes
                .iter()
                .position(|hole| hole.length >= bytes.len())
            {
                return Ok(Self::write(index, block, hole, bytes));
            }
        }
        let remaining = self.maximum.saturating_sub(self.reserved());
        if remaining < bytes.len() {
            return Err(SearchError::LimitExceeded);
        }
        let requested = self.block_size.max(bytes.len()).min(remaining);
        let mut region = match ProtectedBytes::try_with_capacity(requested) {
            Ok(region) => region,
            Err(error)
                if error.kind() == std::io::ErrorKind::PermissionDenied
                    && requested > bytes.len() =>
            {
                ProtectedBytes::try_with_capacity(bytes.len()).map_err(allocation_error)?
            }
            Err(error) => return Err(allocation_error(error)),
        };
        if region.capacity() > remaining {
            return Err(SearchError::LimitExceeded);
        }
        region.fill_capacity();
        self.generation = self
            .generation
            .checked_add(1)
            .ok_or(SearchError::LimitExceeded)?;
        let mut block = Block {
            holes: vec![Hole {
                offset: 0,
                length: region.len(),
            }],
            bytes: region,
            live: 0,
            generation: self.generation,
        };
        let index = self
            .blocks
            .iter()
            .position(Option::is_none)
            .unwrap_or(self.blocks.len());
        let span = Self::write(index, &mut block, 0, bytes);
        if index == self.blocks.len() {
            self.blocks.push(Some(block));
        } else {
            self.blocks[index] = Some(block);
        }
        Ok(span)
    }

    fn write(index: usize, block: &mut Block, hole: usize, bytes: &[u8]) -> Span {
        let offset = block.holes[hole].offset;
        block.bytes.as_mut_slice()[offset..offset + bytes.len()].copy_from_slice(bytes);
        block.holes[hole].offset += bytes.len();
        block.holes[hole].length -= bytes.len();
        if block.holes[hole].length == 0 {
            block.holes.remove(hole);
        }
        block.live += bytes.len();
        Span {
            block: index,
            offset,
            length: bytes.len(),
            generation: block.generation,
        }
    }

    pub fn get(&self, span: Span) -> Option<&[u8]> {
        if span.length == 0 {
            return Some(&[]);
        }
        let block = self.blocks.get(span.block)?.as_ref()?;
        if block.generation != span.generation {
            return None;
        }
        block
            .bytes
            .as_slice()
            .get(span.offset..span.offset.checked_add(span.length)?)
    }

    pub fn remove(&mut self, span: Span) {
        if span.length == 0 {
            return;
        }
        let Some(Some(block)) = self.blocks.get_mut(span.block) else {
            return;
        };
        if block.generation != span.generation {
            return;
        }
        let Some(end) = span.offset.checked_add(span.length) else {
            return;
        };
        let Some(bytes) = block.bytes.as_mut_slice().get_mut(span.offset..end) else {
            return;
        };
        bytes.zeroize();
        block.live = block.live.saturating_sub(span.length);
        if block.live == 0 {
            self.blocks[span.block] = None;
            return;
        }
        block.holes.push(Hole {
            offset: span.offset,
            length: span.length,
        });
        block.holes.sort_unstable_by_key(|hole| hole.offset);
        let mut index = 1;
        while index < block.holes.len() {
            if block.holes[index - 1].offset + block.holes[index - 1].length
                == block.holes[index].offset
            {
                block.holes[index - 1].length += block.holes.remove(index).length;
            } else {
                index += 1;
            }
        }
    }

    pub fn clear(&mut self) {
        self.blocks.clear();
    }

    pub fn reserved(&self) -> usize {
        self.blocks
            .iter()
            .flatten()
            .map(|block| block.bytes.capacity())
            .sum()
    }

    pub fn live(&self) -> usize {
        self.blocks.iter().flatten().map(|block| block.live).sum()
    }

    pub fn regions(&self) -> usize {
        self.blocks.iter().flatten().count()
    }

    pub fn metadata_bytes(&self) -> usize {
        self.blocks.capacity() * size_of::<Option<Block>>()
            + self
                .blocks
                .iter()
                .flatten()
                .map(|block| block.holes.capacity() * size_of::<Hole>())
                .sum::<usize>()
    }
}
