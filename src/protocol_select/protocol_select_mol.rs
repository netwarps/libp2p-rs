// Generated by Molecule 0.6.0

use molecule::prelude::*;
#[derive(Clone)]
pub struct String(molecule::bytes::Bytes);
impl ::core::fmt::LowerHex for String {
    fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
        use molecule::hex_string;
        if f.alternate() {
            write!(f, "0x")?;
        }
        write!(f, "{}", hex_string(self.as_slice()))
    }
}
impl ::core::fmt::Debug for String {
    fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
        write!(f, "{}({:#x})", Self::NAME, self)
    }
}
impl ::core::fmt::Display for String {
    fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
        use molecule::hex_string;
        let raw_data = hex_string(&self.raw_data());
        write!(f, "{}(0x{})", Self::NAME, raw_data)
    }
}
impl ::core::default::Default for String {
    fn default() -> Self {
        let v: Vec<u8> = vec![0, 0, 0, 0];
        String::new_unchecked(v.into())
    }
}
impl String {
    pub const ITEM_SIZE: usize = 1;
    pub fn total_size(&self) -> usize {
        molecule::NUMBER_SIZE * (self.item_count() + 1)
    }
    pub fn item_count(&self) -> usize {
        molecule::unpack_number(self.as_slice()) as usize
    }
    pub fn len(&self) -> usize {
        self.item_count()
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn get(&self, idx: usize) -> Option<Byte> {
        if idx >= self.len() {
            None
        } else {
            Some(self.get_unchecked(idx))
        }
    }
    pub fn get_unchecked(&self, idx: usize) -> Byte {
        let start = molecule::NUMBER_SIZE + Self::ITEM_SIZE * idx;
        let end = start + Self::ITEM_SIZE;
        Byte::new_unchecked(self.0.slice(start..end))
    }
    pub fn raw_data(&self) -> molecule::bytes::Bytes {
        self.0.slice(molecule::NUMBER_SIZE..)
    }
    pub fn as_reader<'r>(&'r self) -> StringReader<'r> {
        StringReader::new_unchecked(self.as_slice())
    }
}
impl molecule::prelude::Entity for String {
    type Builder = StringBuilder;
    const NAME: &'static str = "String";
    fn new_unchecked(data: molecule::bytes::Bytes) -> Self {
        String(data)
    }
    fn as_bytes(&self) -> molecule::bytes::Bytes {
        self.0.clone()
    }
    fn as_slice(&self) -> &[u8] {
        &self.0[..]
    }
    fn from_slice(slice: &[u8]) -> molecule::error::VerificationResult<Self> {
        StringReader::from_slice(slice).map(|reader| reader.to_entity())
    }
    fn from_compatible_slice(slice: &[u8]) -> molecule::error::VerificationResult<Self> {
        StringReader::from_compatible_slice(slice).map(|reader| reader.to_entity())
    }
    fn new_builder() -> Self::Builder {
        ::core::default::Default::default()
    }
    fn as_builder(self) -> Self::Builder {
        Self::new_builder().extend(self.into_iter())
    }
}
#[derive(Clone, Copy)]
pub struct StringReader<'r>(&'r [u8]);
impl<'r> ::core::fmt::LowerHex for StringReader<'r> {
    fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
        use molecule::hex_string;
        if f.alternate() {
            write!(f, "0x")?;
        }
        write!(f, "{}", hex_string(self.as_slice()))
    }
}
impl<'r> ::core::fmt::Debug for StringReader<'r> {
    fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
        write!(f, "{}({:#x})", Self::NAME, self)
    }
}
impl<'r> ::core::fmt::Display for StringReader<'r> {
    fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
        use molecule::hex_string;
        let raw_data = hex_string(&self.raw_data());
        write!(f, "{}(0x{})", Self::NAME, raw_data)
    }
}
impl<'r> StringReader<'r> {
    pub const ITEM_SIZE: usize = 1;
    pub fn total_size(&self) -> usize {
        molecule::NUMBER_SIZE * (self.item_count() + 1)
    }
    pub fn item_count(&self) -> usize {
        molecule::unpack_number(self.as_slice()) as usize
    }
    pub fn len(&self) -> usize {
        self.item_count()
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn get(&self, idx: usize) -> Option<ByteReader<'r>> {
        if idx >= self.len() {
            None
        } else {
            Some(self.get_unchecked(idx))
        }
    }
    pub fn get_unchecked(&self, idx: usize) -> ByteReader<'r> {
        let start = molecule::NUMBER_SIZE + Self::ITEM_SIZE * idx;
        let end = start + Self::ITEM_SIZE;
        ByteReader::new_unchecked(&self.as_slice()[start..end])
    }
    pub fn raw_data(&self) -> &'r [u8] {
        &self.as_slice()[molecule::NUMBER_SIZE..]
    }
}
impl<'r> molecule::prelude::Reader<'r> for StringReader<'r> {
    type Entity = String;
    const NAME: &'static str = "StringReader";
    fn to_entity(&self) -> Self::Entity {
        Self::Entity::new_unchecked(self.as_slice().to_owned().into())
    }
    fn new_unchecked(slice: &'r [u8]) -> Self {
        StringReader(slice)
    }
    fn as_slice(&self) -> &'r [u8] {
        self.0
    }
    fn verify(slice: &[u8], _compatible: bool) -> molecule::error::VerificationResult<()> {
        use molecule::verification_error as ve;
        let slice_len = slice.len();
        if slice_len < molecule::NUMBER_SIZE {
            return ve!(Self, HeaderIsBroken, molecule::NUMBER_SIZE, slice_len);
        }
        let item_count = molecule::unpack_number(slice) as usize;
        if item_count == 0 {
            if slice_len != molecule::NUMBER_SIZE {
                return ve!(Self, TotalSizeNotMatch, molecule::NUMBER_SIZE, slice_len);
            }
            return Ok(());
        }
        let total_size = molecule::NUMBER_SIZE + Self::ITEM_SIZE * item_count;
        if slice_len != total_size {
            return ve!(Self, TotalSizeNotMatch, total_size, slice_len);
        }
        Ok(())
    }
}
#[derive(Debug, Default)]
pub struct StringBuilder(pub(crate) Vec<Byte>);
impl StringBuilder {
    pub const ITEM_SIZE: usize = 1;
    pub fn set(mut self, v: Vec<Byte>) -> Self {
        self.0 = v;
        self
    }
    pub fn push(mut self, v: Byte) -> Self {
        self.0.push(v);
        self
    }
    pub fn extend<T: ::core::iter::IntoIterator<Item = Byte>>(mut self, iter: T) -> Self {
        for elem in iter {
            self.0.push(elem);
        }
        self
    }
}
impl molecule::prelude::Builder for StringBuilder {
    type Entity = String;
    const NAME: &'static str = "StringBuilder";
    fn expected_length(&self) -> usize {
        molecule::NUMBER_SIZE + Self::ITEM_SIZE * self.0.len()
    }
    fn write<W: ::molecule::io::Write>(&self, writer: &mut W) -> ::molecule::io::Result<()> {
        writer.write_all(&molecule::pack_number(self.0.len() as molecule::Number))?;
        for inner in &self.0[..] {
            writer.write_all(inner.as_slice())?;
        }
        Ok(())
    }
    fn build(&self) -> Self::Entity {
        let mut inner = Vec::with_capacity(self.expected_length());
        self.write(&mut inner)
            .unwrap_or_else(|_| panic!("{} build should be ok", Self::NAME));
        String::new_unchecked(inner.into())
    }
}
pub struct StringIterator(String, usize, usize);
impl ::core::iter::Iterator for StringIterator {
    type Item = Byte;
    fn next(&mut self) -> Option<Self::Item> {
        if self.1 >= self.2 {
            None
        } else {
            let ret = self.0.get_unchecked(self.1);
            self.1 += 1;
            Some(ret)
        }
    }
}
impl ::core::iter::ExactSizeIterator for StringIterator {
    fn len(&self) -> usize {
        self.2 - self.1
    }
}
impl ::core::iter::IntoIterator for String {
    type Item = Byte;
    type IntoIter = StringIterator;
    fn into_iter(self) -> Self::IntoIter {
        let len = self.len();
        StringIterator(self, 0, len)
    }
}
#[derive(Clone)]
pub struct StringVec(molecule::bytes::Bytes);
impl ::core::fmt::LowerHex for StringVec {
    fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
        use molecule::hex_string;
        if f.alternate() {
            write!(f, "0x")?;
        }
        write!(f, "{}", hex_string(self.as_slice()))
    }
}
impl ::core::fmt::Debug for StringVec {
    fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
        write!(f, "{}({:#x})", Self::NAME, self)
    }
}
impl ::core::fmt::Display for StringVec {
    fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
        write!(f, "{} [", Self::NAME)?;
        for i in 0..self.len() {
            if i == 0 {
                write!(f, "{}", self.get_unchecked(i))?;
            } else {
                write!(f, ", {}", self.get_unchecked(i))?;
            }
        }
        write!(f, "]")
    }
}
impl ::core::default::Default for StringVec {
    fn default() -> Self {
        let v: Vec<u8> = vec![4, 0, 0, 0];
        StringVec::new_unchecked(v.into())
    }
}
impl StringVec {
    pub fn total_size(&self) -> usize {
        molecule::unpack_number(self.as_slice()) as usize
    }
    pub fn item_count(&self) -> usize {
        if self.total_size() == molecule::NUMBER_SIZE {
            0
        } else {
            (molecule::unpack_number(&self.as_slice()[molecule::NUMBER_SIZE..]) as usize / 4) - 1
        }
    }
    pub fn len(&self) -> usize {
        self.item_count()
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn get(&self, idx: usize) -> Option<String> {
        if idx >= self.len() {
            None
        } else {
            Some(self.get_unchecked(idx))
        }
    }
    pub fn get_unchecked(&self, idx: usize) -> String {
        let slice = self.as_slice();
        let start_idx = molecule::NUMBER_SIZE * (1 + idx);
        let start = molecule::unpack_number(&slice[start_idx..]) as usize;
        if idx == self.len() - 1 {
            String::new_unchecked(self.0.slice(start..))
        } else {
            let end_idx = start_idx + molecule::NUMBER_SIZE;
            let end = molecule::unpack_number(&slice[end_idx..]) as usize;
            String::new_unchecked(self.0.slice(start..end))
        }
    }
    pub fn as_reader<'r>(&'r self) -> StringVecReader<'r> {
        StringVecReader::new_unchecked(self.as_slice())
    }
}
impl molecule::prelude::Entity for StringVec {
    type Builder = StringVecBuilder;
    const NAME: &'static str = "StringVec";
    fn new_unchecked(data: molecule::bytes::Bytes) -> Self {
        StringVec(data)
    }
    fn as_bytes(&self) -> molecule::bytes::Bytes {
        self.0.clone()
    }
    fn as_slice(&self) -> &[u8] {
        &self.0[..]
    }
    fn from_slice(slice: &[u8]) -> molecule::error::VerificationResult<Self> {
        StringVecReader::from_slice(slice).map(|reader| reader.to_entity())
    }
    fn from_compatible_slice(slice: &[u8]) -> molecule::error::VerificationResult<Self> {
        StringVecReader::from_compatible_slice(slice).map(|reader| reader.to_entity())
    }
    fn new_builder() -> Self::Builder {
        ::core::default::Default::default()
    }
    fn as_builder(self) -> Self::Builder {
        Self::new_builder().extend(self.into_iter())
    }
}
#[derive(Clone, Copy)]
pub struct StringVecReader<'r>(&'r [u8]);
impl<'r> ::core::fmt::LowerHex for StringVecReader<'r> {
    fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
        use molecule::hex_string;
        if f.alternate() {
            write!(f, "0x")?;
        }
        write!(f, "{}", hex_string(self.as_slice()))
    }
}
impl<'r> ::core::fmt::Debug for StringVecReader<'r> {
    fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
        write!(f, "{}({:#x})", Self::NAME, self)
    }
}
impl<'r> ::core::fmt::Display for StringVecReader<'r> {
    fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
        write!(f, "{} [", Self::NAME)?;
        for i in 0..self.len() {
            if i == 0 {
                write!(f, "{}", self.get_unchecked(i))?;
            } else {
                write!(f, ", {}", self.get_unchecked(i))?;
            }
        }
        write!(f, "]")
    }
}
impl<'r> StringVecReader<'r> {
    pub fn total_size(&self) -> usize {
        molecule::unpack_number(self.as_slice()) as usize
    }
    pub fn item_count(&self) -> usize {
        if self.total_size() == molecule::NUMBER_SIZE {
            0
        } else {
            (molecule::unpack_number(&self.as_slice()[molecule::NUMBER_SIZE..]) as usize / 4) - 1
        }
    }
    pub fn len(&self) -> usize {
        self.item_count()
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn get(&self, idx: usize) -> Option<StringReader<'r>> {
        if idx >= self.len() {
            None
        } else {
            Some(self.get_unchecked(idx))
        }
    }
    pub fn get_unchecked(&self, idx: usize) -> StringReader<'r> {
        let slice = self.as_slice();
        let start_idx = molecule::NUMBER_SIZE * (1 + idx);
        let start = molecule::unpack_number(&slice[start_idx..]) as usize;
        if idx == self.len() - 1 {
            StringReader::new_unchecked(&self.as_slice()[start..])
        } else {
            let end_idx = start_idx + molecule::NUMBER_SIZE;
            let end = molecule::unpack_number(&slice[end_idx..]) as usize;
            StringReader::new_unchecked(&self.as_slice()[start..end])
        }
    }
}
impl<'r> molecule::prelude::Reader<'r> for StringVecReader<'r> {
    type Entity = StringVec;
    const NAME: &'static str = "StringVecReader";
    fn to_entity(&self) -> Self::Entity {
        Self::Entity::new_unchecked(self.as_slice().to_owned().into())
    }
    fn new_unchecked(slice: &'r [u8]) -> Self {
        StringVecReader(slice)
    }
    fn as_slice(&self) -> &'r [u8] {
        self.0
    }
    fn verify(slice: &[u8], compatible: bool) -> molecule::error::VerificationResult<()> {
        use molecule::verification_error as ve;
        let slice_len = slice.len();
        if slice_len < molecule::NUMBER_SIZE {
            return ve!(Self, HeaderIsBroken, molecule::NUMBER_SIZE, slice_len);
        }
        let total_size = molecule::unpack_number(slice) as usize;
        if slice_len != total_size {
            return ve!(Self, TotalSizeNotMatch, total_size, slice_len);
        }
        if slice_len == molecule::NUMBER_SIZE {
            return Ok(());
        }
        if slice_len < molecule::NUMBER_SIZE * 2 {
            return ve!(
                Self,
                TotalSizeNotMatch,
                molecule::NUMBER_SIZE * 2,
                slice_len
            );
        }
        let offset_first = molecule::unpack_number(&slice[molecule::NUMBER_SIZE..]) as usize;
        if offset_first % 4 != 0 || offset_first < molecule::NUMBER_SIZE * 2 {
            return ve!(Self, OffsetsNotMatch);
        }
        let item_count = offset_first / 4 - 1;
        let header_size = molecule::NUMBER_SIZE * (item_count + 1);
        if slice_len < header_size {
            return ve!(Self, HeaderIsBroken, header_size, slice_len);
        }
        let mut offsets: Vec<usize> = slice[molecule::NUMBER_SIZE..]
            .chunks(molecule::NUMBER_SIZE)
            .take(item_count)
            .map(|x| molecule::unpack_number(x) as usize)
            .collect();
        offsets.push(total_size);
        if offsets.windows(2).any(|i| i[0] > i[1]) {
            return ve!(Self, OffsetsNotMatch);
        }
        for pair in offsets.windows(2) {
            let start = pair[0];
            let end = pair[1];
            StringReader::verify(&slice[start..end], compatible)?;
        }
        Ok(())
    }
}
#[derive(Debug, Default)]
pub struct StringVecBuilder(pub(crate) Vec<String>);
impl StringVecBuilder {
    pub fn set(mut self, v: Vec<String>) -> Self {
        self.0 = v;
        self
    }
    pub fn push(mut self, v: String) -> Self {
        self.0.push(v);
        self
    }
    pub fn extend<T: ::core::iter::IntoIterator<Item = String>>(mut self, iter: T) -> Self {
        for elem in iter {
            self.0.push(elem);
        }
        self
    }
}
impl molecule::prelude::Builder for StringVecBuilder {
    type Entity = StringVec;
    const NAME: &'static str = "StringVecBuilder";
    fn expected_length(&self) -> usize {
        molecule::NUMBER_SIZE * (self.0.len() + 1)
            + self
                .0
                .iter()
                .map(|inner| inner.as_slice().len())
                .sum::<usize>()
    }
    fn write<W: ::molecule::io::Write>(&self, writer: &mut W) -> ::molecule::io::Result<()> {
        let item_count = self.0.len();
        if item_count == 0 {
            writer.write_all(&molecule::pack_number(
                molecule::NUMBER_SIZE as molecule::Number,
            ))?;
        } else {
            let (total_size, offsets) = self.0.iter().fold(
                (
                    molecule::NUMBER_SIZE * (item_count + 1),
                    Vec::with_capacity(item_count),
                ),
                |(start, mut offsets), inner| {
                    offsets.push(start);
                    (start + inner.as_slice().len(), offsets)
                },
            );
            writer.write_all(&molecule::pack_number(total_size as molecule::Number))?;
            for offset in offsets.into_iter() {
                writer.write_all(&molecule::pack_number(offset as molecule::Number))?;
            }
            for inner in self.0.iter() {
                writer.write_all(inner.as_slice())?;
            }
        }
        Ok(())
    }
    fn build(&self) -> Self::Entity {
        let mut inner = Vec::with_capacity(self.expected_length());
        self.write(&mut inner)
            .unwrap_or_else(|_| panic!("{} build should be ok", Self::NAME));
        StringVec::new_unchecked(inner.into())
    }
}
pub struct StringVecIterator(StringVec, usize, usize);
impl ::core::iter::Iterator for StringVecIterator {
    type Item = String;
    fn next(&mut self) -> Option<Self::Item> {
        if self.1 >= self.2 {
            None
        } else {
            let ret = self.0.get_unchecked(self.1);
            self.1 += 1;
            Some(ret)
        }
    }
}
impl ::core::iter::ExactSizeIterator for StringVecIterator {
    fn len(&self) -> usize {
        self.2 - self.1
    }
}
impl ::core::iter::IntoIterator for StringVec {
    type Item = String;
    type IntoIter = StringVecIterator;
    fn into_iter(self) -> Self::IntoIter {
        let len = self.len();
        StringVecIterator(self, 0, len)
    }
}
impl<'r> StringVecReader<'r> {
    pub fn iter<'t>(&'t self) -> StringVecReaderIterator<'t, 'r> {
        StringVecReaderIterator(&self, 0, self.len())
    }
}
pub struct StringVecReaderIterator<'t, 'r>(&'t StringVecReader<'r>, usize, usize);
impl<'t: 'r, 'r> ::core::iter::Iterator for StringVecReaderIterator<'t, 'r> {
    type Item = StringReader<'t>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.1 >= self.2 {
            None
        } else {
            let ret = self.0.get_unchecked(self.1);
            self.1 += 1;
            Some(ret)
        }
    }
}
impl<'t: 'r, 'r> ::core::iter::ExactSizeIterator for StringVecReaderIterator<'t, 'r> {
    fn len(&self) -> usize {
        self.2 - self.1
    }
}
#[derive(Clone)]
pub struct ProtocolInfo(molecule::bytes::Bytes);
impl ::core::fmt::LowerHex for ProtocolInfo {
    fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
        use molecule::hex_string;
        if f.alternate() {
            write!(f, "0x")?;
        }
        write!(f, "{}", hex_string(self.as_slice()))
    }
}
impl ::core::fmt::Debug for ProtocolInfo {
    fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
        write!(f, "{}({:#x})", Self::NAME, self)
    }
}
impl ::core::fmt::Display for ProtocolInfo {
    fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
        write!(f, "{} {{ ", Self::NAME)?;
        write!(f, "{}: {}", "name", self.name())?;
        write!(f, ", {}: {}", "support_versions", self.support_versions())?;
        let extra_count = self.count_extra_fields();
        if extra_count != 0 {
            write!(f, ", .. ({} fields)", extra_count)?;
        }
        write!(f, " }}")
    }
}
impl ::core::default::Default for ProtocolInfo {
    fn default() -> Self {
        let v: Vec<u8> = vec![
            20, 0, 0, 0, 12, 0, 0, 0, 16, 0, 0, 0, 0, 0, 0, 0, 4, 0, 0, 0,
        ];
        ProtocolInfo::new_unchecked(v.into())
    }
}
impl ProtocolInfo {
    pub const FIELD_COUNT: usize = 2;
    pub fn total_size(&self) -> usize {
        molecule::unpack_number(self.as_slice()) as usize
    }
    pub fn field_count(&self) -> usize {
        if self.total_size() == molecule::NUMBER_SIZE {
            0
        } else {
            (molecule::unpack_number(&self.as_slice()[molecule::NUMBER_SIZE..]) as usize / 4) - 1
        }
    }
    pub fn count_extra_fields(&self) -> usize {
        self.field_count() - Self::FIELD_COUNT
    }
    pub fn has_extra_fields(&self) -> bool {
        Self::FIELD_COUNT != self.field_count()
    }
    pub fn name(&self) -> String {
        let slice = self.as_slice();
        let start = molecule::unpack_number(&slice[4..]) as usize;
        let end = molecule::unpack_number(&slice[8..]) as usize;
        String::new_unchecked(self.0.slice(start..end))
    }
    pub fn support_versions(&self) -> StringVec {
        let slice = self.as_slice();
        let start = molecule::unpack_number(&slice[8..]) as usize;
        if self.has_extra_fields() {
            let end = molecule::unpack_number(&slice[12..]) as usize;
            StringVec::new_unchecked(self.0.slice(start..end))
        } else {
            StringVec::new_unchecked(self.0.slice(start..))
        }
    }
    pub fn as_reader<'r>(&'r self) -> ProtocolInfoReader<'r> {
        ProtocolInfoReader::new_unchecked(self.as_slice())
    }
}
impl molecule::prelude::Entity for ProtocolInfo {
    type Builder = ProtocolInfoBuilder;
    const NAME: &'static str = "ProtocolInfo";
    fn new_unchecked(data: molecule::bytes::Bytes) -> Self {
        ProtocolInfo(data)
    }
    fn as_bytes(&self) -> molecule::bytes::Bytes {
        self.0.clone()
    }
    fn as_slice(&self) -> &[u8] {
        &self.0[..]
    }
    fn from_slice(slice: &[u8]) -> molecule::error::VerificationResult<Self> {
        ProtocolInfoReader::from_slice(slice).map(|reader| reader.to_entity())
    }
    fn from_compatible_slice(slice: &[u8]) -> molecule::error::VerificationResult<Self> {
        ProtocolInfoReader::from_compatible_slice(slice).map(|reader| reader.to_entity())
    }
    fn new_builder() -> Self::Builder {
        ::core::default::Default::default()
    }
    fn as_builder(self) -> Self::Builder {
        Self::new_builder()
            .name(self.name())
            .support_versions(self.support_versions())
    }
}
#[derive(Clone, Copy)]
pub struct ProtocolInfoReader<'r>(&'r [u8]);
impl<'r> ::core::fmt::LowerHex for ProtocolInfoReader<'r> {
    fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
        use molecule::hex_string;
        if f.alternate() {
            write!(f, "0x")?;
        }
        write!(f, "{}", hex_string(self.as_slice()))
    }
}
impl<'r> ::core::fmt::Debug for ProtocolInfoReader<'r> {
    fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
        write!(f, "{}({:#x})", Self::NAME, self)
    }
}
impl<'r> ::core::fmt::Display for ProtocolInfoReader<'r> {
    fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
        write!(f, "{} {{ ", Self::NAME)?;
        write!(f, "{}: {}", "name", self.name())?;
        write!(f, ", {}: {}", "support_versions", self.support_versions())?;
        let extra_count = self.count_extra_fields();
        if extra_count != 0 {
            write!(f, ", .. ({} fields)", extra_count)?;
        }
        write!(f, " }}")
    }
}
impl<'r> ProtocolInfoReader<'r> {
    pub const FIELD_COUNT: usize = 2;
    pub fn total_size(&self) -> usize {
        molecule::unpack_number(self.as_slice()) as usize
    }
    pub fn field_count(&self) -> usize {
        if self.total_size() == molecule::NUMBER_SIZE {
            0
        } else {
            (molecule::unpack_number(&self.as_slice()[molecule::NUMBER_SIZE..]) as usize / 4) - 1
        }
    }
    pub fn count_extra_fields(&self) -> usize {
        self.field_count() - Self::FIELD_COUNT
    }
    pub fn has_extra_fields(&self) -> bool {
        Self::FIELD_COUNT != self.field_count()
    }
    pub fn name(&self) -> StringReader<'r> {
        let slice = self.as_slice();
        let start = molecule::unpack_number(&slice[4..]) as usize;
        let end = molecule::unpack_number(&slice[8..]) as usize;
        StringReader::new_unchecked(&self.as_slice()[start..end])
    }
    pub fn support_versions(&self) -> StringVecReader<'r> {
        let slice = self.as_slice();
        let start = molecule::unpack_number(&slice[8..]) as usize;
        if self.has_extra_fields() {
            let end = molecule::unpack_number(&slice[12..]) as usize;
            StringVecReader::new_unchecked(&self.as_slice()[start..end])
        } else {
            StringVecReader::new_unchecked(&self.as_slice()[start..])
        }
    }
}
impl<'r> molecule::prelude::Reader<'r> for ProtocolInfoReader<'r> {
    type Entity = ProtocolInfo;
    const NAME: &'static str = "ProtocolInfoReader";
    fn to_entity(&self) -> Self::Entity {
        Self::Entity::new_unchecked(self.as_slice().to_owned().into())
    }
    fn new_unchecked(slice: &'r [u8]) -> Self {
        ProtocolInfoReader(slice)
    }
    fn as_slice(&self) -> &'r [u8] {
        self.0
    }
    fn verify(slice: &[u8], compatible: bool) -> molecule::error::VerificationResult<()> {
        use molecule::verification_error as ve;
        let slice_len = slice.len();
        if slice_len < molecule::NUMBER_SIZE {
            return ve!(Self, HeaderIsBroken, molecule::NUMBER_SIZE, slice_len);
        }
        let total_size = molecule::unpack_number(slice) as usize;
        if slice_len != total_size {
            return ve!(Self, TotalSizeNotMatch, total_size, slice_len);
        }
        if slice_len == molecule::NUMBER_SIZE && Self::FIELD_COUNT == 0 {
            return Ok(());
        }
        if slice_len < molecule::NUMBER_SIZE * 2 {
            return ve!(Self, HeaderIsBroken, molecule::NUMBER_SIZE * 2, slice_len);
        }
        let offset_first = molecule::unpack_number(&slice[molecule::NUMBER_SIZE..]) as usize;
        if offset_first % 4 != 0 || offset_first < molecule::NUMBER_SIZE * 2 {
            return ve!(Self, OffsetsNotMatch);
        }
        let field_count = offset_first / 4 - 1;
        if field_count < Self::FIELD_COUNT {
            return ve!(Self, FieldCountNotMatch, Self::FIELD_COUNT, field_count);
        } else if !compatible && field_count > Self::FIELD_COUNT {
            return ve!(Self, FieldCountNotMatch, Self::FIELD_COUNT, field_count);
        };
        let header_size = molecule::NUMBER_SIZE * (field_count + 1);
        if slice_len < header_size {
            return ve!(Self, HeaderIsBroken, header_size, slice_len);
        }
        let mut offsets: Vec<usize> = slice[molecule::NUMBER_SIZE..]
            .chunks(molecule::NUMBER_SIZE)
            .take(field_count)
            .map(|x| molecule::unpack_number(x) as usize)
            .collect();
        offsets.push(total_size);
        if offsets.windows(2).any(|i| i[0] > i[1]) {
            return ve!(Self, OffsetsNotMatch);
        }
        StringReader::verify(&slice[offsets[0]..offsets[1]], compatible)?;
        StringVecReader::verify(&slice[offsets[1]..offsets[2]], compatible)?;
        Ok(())
    }
}
#[derive(Debug, Default)]
pub struct ProtocolInfoBuilder {
    pub(crate) name: String,
    pub(crate) support_versions: StringVec,
}
impl ProtocolInfoBuilder {
    pub const FIELD_COUNT: usize = 2;
    pub fn name(mut self, v: String) -> Self {
        self.name = v;
        self
    }
    pub fn support_versions(mut self, v: StringVec) -> Self {
        self.support_versions = v;
        self
    }
}
impl molecule::prelude::Builder for ProtocolInfoBuilder {
    type Entity = ProtocolInfo;
    const NAME: &'static str = "ProtocolInfoBuilder";
    fn expected_length(&self) -> usize {
        molecule::NUMBER_SIZE * (Self::FIELD_COUNT + 1)
            + self.name.as_slice().len()
            + self.support_versions.as_slice().len()
    }
    fn write<W: ::molecule::io::Write>(&self, writer: &mut W) -> ::molecule::io::Result<()> {
        let mut total_size = molecule::NUMBER_SIZE * (Self::FIELD_COUNT + 1);
        let mut offsets = Vec::with_capacity(Self::FIELD_COUNT);
        offsets.push(total_size);
        total_size += self.name.as_slice().len();
        offsets.push(total_size);
        total_size += self.support_versions.as_slice().len();
        writer.write_all(&molecule::pack_number(total_size as molecule::Number))?;
        for offset in offsets.into_iter() {
            writer.write_all(&molecule::pack_number(offset as molecule::Number))?;
        }
        writer.write_all(self.name.as_slice())?;
        writer.write_all(self.support_versions.as_slice())?;
        Ok(())
    }
    fn build(&self) -> Self::Entity {
        let mut inner = Vec::with_capacity(self.expected_length());
        self.write(&mut inner)
            .unwrap_or_else(|_| panic!("{} build should be ok", Self::NAME));
        ProtocolInfo::new_unchecked(inner.into())
    }
}
