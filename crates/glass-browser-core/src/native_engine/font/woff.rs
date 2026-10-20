use super::*;

#[derive(Debug, Clone, Copy)]
struct WoffTable {
    tag: u32,
    offset: usize,
    compressed_len: usize,
    original_len: usize,
    checksum: u32,
}

pub(super) fn read_u16_be(bytes: &[u8], offset: usize) -> Option<u16> {
    let value = bytes.get(offset..offset.checked_add(2)?)?;
    Some(u16::from_be_bytes(value.try_into().ok()?))
}

pub(super) fn read_u32_be(bytes: &[u8], offset: usize) -> Option<u32> {
    let value = bytes.get(offset..offset.checked_add(4)?)?;
    Some(u32::from_be_bytes(value.try_into().ok()?))
}

pub(super) fn write_u16_be(bytes: &mut [u8], offset: usize, value: u16) {
    if let Some(slot) = bytes.get_mut(offset..offset.saturating_add(2))
        && slot.len() == 2
    {
        slot.copy_from_slice(&value.to_be_bytes());
    }
}

pub(super) fn write_u32_be(bytes: &mut [u8], offset: usize, value: u32) {
    if let Some(slot) = bytes.get_mut(offset..offset.saturating_add(4))
        && slot.len() == 4
    {
        slot.copy_from_slice(&value.to_be_bytes());
    }
}

pub(super) fn supported_woff_flavor(flavor: u32) -> bool {
    matches!(
        flavor,
        0x0001_0000 | 0x4f54_544f | 0x7472_7565 | 0x7479_7031 | 0x7474_6366
    )
}

fn valid_woff_range(offset: usize, length: usize, total_length: usize) -> bool {
    offset
        .checked_add(length)
        .is_some_and(|end| offset <= total_length && end <= total_length)
}

fn sfnt_checksum(bytes: &[u8]) -> u32 {
    bytes
        .chunks(4)
        .map(|chunk| {
            let mut word = [0; 4];
            word[..chunk.len()].copy_from_slice(chunk);
            u32::from_be_bytes(word)
        })
        .fold(0, u32::wrapping_add)
}

/// Convert a bounded WOFF 1.0 container to the SFNT bytes consumed by the
/// existing fontdue/HarfRust owners.
fn decode_woff(bytes: &[u8]) -> Option<Vec<u8>> {
    if bytes.len() < WOFF_HEADER_BYTES || bytes.len() > MAX_NATIVE_FONT_BYTES {
        return None;
    }
    let flavor = read_u32_be(bytes, 4)?;
    if !supported_woff_flavor(flavor) {
        return None;
    }
    let declared_length = usize::try_from(read_u32_be(bytes, 8)?).ok()?;
    if declared_length != bytes.len() {
        return None;
    }
    let table_count = usize::from(read_u16_be(bytes, 12)?);
    if table_count == 0 || table_count > MAX_WOFF_TABLES {
        return None;
    }
    let total_sfnt_size = usize::try_from(read_u32_be(bytes, 16)?).ok()?;
    if total_sfnt_size == 0 || total_sfnt_size > MAX_NATIVE_FONT_BYTES {
        return None;
    }
    for (offset_index, length_index) in [(24, 28), (36, 40)] {
        let offset = usize::try_from(read_u32_be(bytes, offset_index)?).ok()?;
        let length = usize::try_from(read_u32_be(bytes, length_index)?).ok()?;
        if length > 0 && !valid_woff_range(offset, length, declared_length) {
            return None;
        }
    }
    let table_directory_end =
        WOFF_HEADER_BYTES.checked_add(table_count.checked_mul(WOFF_TABLE_BYTES)?)?;
    if table_directory_end > declared_length {
        return None;
    }
    let mut tables = Vec::with_capacity(table_count);
    for index in 0..table_count {
        let entry = WOFF_HEADER_BYTES.checked_add(index.checked_mul(WOFF_TABLE_BYTES)?)?;
        let offset = usize::try_from(read_u32_be(bytes, entry.checked_add(4)?)?).ok()?;
        let compressed_len = usize::try_from(read_u32_be(bytes, entry.checked_add(8)?)?).ok()?;
        let original_len = usize::try_from(read_u32_be(bytes, entry.checked_add(12)?)?).ok()?;
        if compressed_len == 0
            || original_len == 0
            || compressed_len > original_len
            || offset < table_directory_end
            || !valid_woff_range(offset, compressed_len, declared_length)
        {
            return None;
        }
        tables.push(WoffTable {
            tag: read_u32_be(bytes, entry)?,
            offset,
            compressed_len,
            original_len,
            checksum: read_u32_be(bytes, entry.checked_add(16)?)?,
        });
    }
    let mut ranges = tables
        .iter()
        .map(|table| {
            Some((
                table.offset,
                table.offset.checked_add(table.compressed_len)?,
            ))
        })
        .collect::<Option<Vec<_>>>()?;
    ranges.sort_unstable();
    if ranges.windows(2).any(|pair| pair[0].1 > pair[1].0) {
        return None;
    }
    tables.sort_unstable_by_key(|table| table.tag);

    let directory_size = 12usize.checked_add(table_count.checked_mul(16)?)?;
    if directory_size > total_sfnt_size {
        return None;
    }
    let original_table_bytes = tables
        .iter()
        .try_fold(0usize, |total, table| total.checked_add(table.original_len))?;
    if original_table_bytes > total_sfnt_size.saturating_sub(directory_size) {
        return None;
    }
    let mut max_power_of_two = 1usize;
    let mut entry_selector = 0u16;
    while max_power_of_two
        .checked_mul(2)
        .is_some_and(|value| value <= table_count)
    {
        max_power_of_two = max_power_of_two.saturating_mul(2);
        entry_selector = entry_selector.saturating_add(1);
    }
    let search_range = u16::try_from(max_power_of_two.checked_mul(16)?).ok()?;
    let range_shift = u16::try_from(
        table_count
            .checked_mul(16)?
            .saturating_sub(usize::from(search_range)),
    )
    .ok()?;
    let mut output = vec![0; directory_size];
    write_u32_be(&mut output, 0, flavor);
    write_u16_be(&mut output, 4, u16::try_from(table_count).ok()?);
    write_u16_be(&mut output, 6, search_range);
    write_u16_be(&mut output, 8, entry_selector);
    write_u16_be(&mut output, 10, range_shift);
    let mut head_offset = None;
    for (index, table) in tables.iter().enumerate() {
        let end = table.offset.checked_add(table.compressed_len)?;
        let mut decoded = Vec::with_capacity(table.original_len);
        if table.compressed_len == table.original_len {
            decoded.extend_from_slice(bytes.get(table.offset..end)?);
        } else {
            let compressed = bytes.get(table.offset..end)?;
            let decoder = flate2::read::ZlibDecoder::new(compressed);
            decoder
                .take(u64::try_from(table.original_len.saturating_add(1)).ok()?)
                .read_to_end(&mut decoded)
                .ok()?;
        }
        if decoded.len() != table.original_len {
            return None;
        }
        let aligned_length = output
            .len()
            .checked_add(3)?
            .checked_div(4)?
            .checked_mul(4)?;
        if aligned_length > total_sfnt_size {
            return None;
        }
        output.resize(aligned_length, 0);
        if decoded.len() > total_sfnt_size.saturating_sub(output.len()) {
            return None;
        }
        let data_offset = output.len();
        output.extend_from_slice(&decoded);
        if output.len() > total_sfnt_size {
            return None;
        }
        let directory_entry = 12usize.checked_add(index.checked_mul(16)?)?;
        write_u32_be(&mut output, directory_entry, table.tag);
        write_u32_be(&mut output, directory_entry.checked_add(4)?, table.checksum);
        write_u32_be(
            &mut output,
            directory_entry.checked_add(8)?,
            u32::try_from(data_offset).ok()?,
        );
        write_u32_be(
            &mut output,
            directory_entry.checked_add(12)?,
            u32::try_from(table.original_len).ok()?,
        );
        if table.tag == u32::from_be_bytes(*b"head") {
            head_offset = Some(data_offset);
        }
    }
    if output.len() != total_sfnt_size {
        return None;
    }
    if let Some(head_offset) = head_offset
        && head_offset
            .checked_add(12)
            .is_some_and(|end| end <= output.len())
    {
        write_u32_be(&mut output, head_offset.checked_add(8)?, 0);
        let adjustment = 0xb1b0_afbau32.wrapping_sub(sfnt_checksum(&output));
        write_u32_be(&mut output, head_offset.checked_add(8)?, adjustment);
    }
    Some(output)
}

struct BoundedBrotliWriter {
    bytes: Vec<u8>,
    limit: usize,
}

impl Write for BoundedBrotliWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > self.limit.saturating_sub(self.bytes.len()) {
            return Err(io::Error::new(
                io::ErrorKind::WriteZero,
                "WOFF2 Brotli output exceeds the native font limit",
            ));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn decode_woff2_brotli(
    compressed: &[u8],
    size_hint: usize,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    if size_hint == 0 || size_hint > MAX_NATIVE_FONT_BYTES {
        return Err(Box::new(io::Error::new(
            io::ErrorKind::InvalidData,
            "WOFF2 Brotli table stream exceeds the native font limit",
        )));
    }
    let mut reader = Cursor::new(compressed);
    let mut writer = BoundedBrotliWriter {
        bytes: Vec::with_capacity(size_hint),
        limit: size_hint,
    };
    brotli_decompressor::BrotliDecompress(&mut reader, &mut writer)?;
    if writer.bytes.len() != size_hint {
        return Err(Box::new(io::Error::new(
            io::ErrorKind::InvalidData,
            "WOFF2 Brotli table stream has an unexpected size",
        )));
    }
    Ok(writer.bytes)
}

/// Convert a bounded WOFF2 container to the SFNT bytes consumed by the
/// existing fontdue/HarfRust owners. Header limits are checked before Wuff's
/// table-directory parser and the Brotli callback keeps its decompressed table
/// stream within the same native font budget.
fn decode_woff2(bytes: &[u8]) -> Option<Vec<u8>> {
    if bytes.len() < WOFF2_HEADER_BYTES || bytes.len() > MAX_NATIVE_FONT_BYTES {
        return None;
    }
    if read_u32_be(bytes, 0)? != 0x774f_4632 {
        return None;
    }
    let declared_length = usize::try_from(read_u32_be(bytes, 8)?).ok()?;
    if declared_length != bytes.len() {
        return None;
    }
    let table_count = usize::from(read_u16_be(bytes, 12)?);
    if table_count == 0 || table_count > MAX_WOFF_TABLES {
        return None;
    }
    let total_sfnt_size = usize::try_from(read_u32_be(bytes, 16)?).ok()?;
    if total_sfnt_size == 0 || total_sfnt_size > MAX_NATIVE_FONT_BYTES {
        return None;
    }
    let total_compressed_size = usize::try_from(read_u32_be(bytes, 20)?).ok()?;
    if total_compressed_size == 0 || total_compressed_size > bytes.len() {
        return None;
    }
    let decoded =
        wuff::decompress_woff2_with_custom_brotli(bytes, &mut decode_woff2_brotli).ok()?;
    (decoded.len() <= MAX_NATIVE_FONT_BYTES).then_some(decoded)
}

pub(super) fn normalize_font_bytes(bytes: &[u8]) -> Option<Vec<u8>> {
    if bytes.is_empty() || bytes.len() > MAX_NATIVE_FONT_BYTES {
        return None;
    }
    match read_u32_be(bytes, 0)? {
        0x774f_4646 => decode_woff(bytes),
        0x774f_4632 => decode_woff2(bytes),
        _ => Some(bytes.to_vec()),
    }
}
