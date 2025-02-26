use std::{fmt::Display, io::Cursor};

use binrw::{BinRead, NullString};

#[repr(C)]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, binrw::BinRead)]
pub struct TypeId(u32);

pub trait RecordEntry {
    const RECORD_TYPE: u16 = 0;
    fn is_valid_record_type(record_type: u16) -> bool {
        Self::RECORD_TYPE == record_type
    }
}

#[derive(Default, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct CodeViewString(Vec<u8>);

impl CodeViewString {
    pub fn new(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }
}

impl Display for CodeViewString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        String::from_utf8_lossy(&self.0).fmt(f)
    }
}

impl std::fmt::Debug for CodeViewString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(&String::from_utf8_lossy(&self.0), f)
    }
}

impl BinRead for CodeViewString {
    type Args<'a> = ();

    fn read_options<R: std::io::Read + std::io::Seek>(
        reader: &mut R,
        endian: binrw::Endian,
        _args: Self::Args<'_>,
    ) -> binrw::BinResult<Self> {
        let length = <u16>::read_options(reader, endian, ())?;
        let result: binrw::BinResult<Vec<u8>> = (0..length as usize)
            .map(|_| <u8>::read_options(reader, endian, ()))
            .collect();
        let result = result?;
        Ok(CodeViewString(result))
    }
}

// unsafe impl bytemuck::CheckedBitPattern for CodeViewString {
//     type Bits;

//     fn is_valid_bit_pattern(bits: &Self::Bits) -> bool {
//         todo!()
//     }
// }

#[repr(C)]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, binrw::BinRead)]
pub struct CompileSym3Flags {
    language: u8, // language index
    flags: u16,
    pad: u8,
    // unsigned long   fEC             :  1,   // compiled for E/C
    // unsigned long   fNoDbgInfo      :  1,   // not compiled with debug info
    // unsigned long   fLTCG           :  1,   // compiled with LTCG
    // unsigned long   fNoDataAlign    :  1,   // compiled with -Bzalign
    // unsigned long   fManagedPresent :  1,   // managed code/data present
    // unsigned long   fSecurityChecks :  1,   // compiled with /GS
    // unsigned long   fHotPatch       :  1,   // compiled with /hotpatch
    // unsigned long   fCVTCIL         :  1,   // converted with CVTCIL
    // unsigned long   fMSILModule     :  1,   // MSIL netmodule
    // unsigned long   fSdl            :  1,   // compiled with /sdl
    // unsigned long   fPGO            :  1,   // compiled with /ltcg:pgo or pgu
    // unsigned long   fExp            :  1,   // .exp module
    // unsigned long   pad             : 12,   // reserved, must be 0
}

#[repr(C)]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, binrw::BinRead)]
pub struct CompileSym1Flags {
    language: u8, // language index
    flags: u16,
    pad: u8,
    // unsigned long   fEC             :  1,   // compiled for E/C
    // unsigned long   fNoDbgInfo      :  1,   // not compiled with debug info
    // unsigned long   fLTCG           :  1,   // compiled with LTCG
    // unsigned long   fNoDataAlign    :  1,   // compiled with -Bzalign
    // unsigned long   fManagedPresent :  1,   // managed code/data present
    // unsigned long   fSecurityChecks :  1,   // compiled with /GS
    // unsigned long   fHotPatch       :  1,   // compiled with /hotpatch
    // unsigned long   fCVTCIL         :  1,   // converted with CVTCIL
    // unsigned long   fMSILModule     :  1,   // MSIL netmodule
    // unsigned long   pad             : 15,   // reserved, must be 0
}

#[repr(C)]
#[derive(Debug, Default, binrw::BinRead)]
pub struct CompileSym3 {
    reclen: u16, // Record length
    rectyp: u16, // S_COMPILE3
    flags: CompileSym3Flags,
    machine: u16,               // target processor
    ver_frontend_major: u16,    // front end major version #
    ver_frontend_eminor: u16,   // front end minor version #
    ver_frontend_build: u16,    // front end build version #
    ver_frontend_qfe: u16,      // front end QFE version #
    ver_major: u16,             // back end major version #
    ver_minor: u16,             // back end minor version #
    ver_build: u16,             // back end build version #
    ver_qfe: u16,               // back end QFE version #
    version: binrw::NullString, // Zero terminated compiler version string
}

impl RecordEntry for CompileSym3 {
    const RECORD_TYPE: u16 = 0x113c;
}

#[repr(C)]
#[derive(Debug, binrw::BinRead)]
pub struct CompileSym1 {
    reclen: u16, // Record length
    rectyp: u16, // S_COMPILE2
    flags: CompileSym1Flags,
    machine: u16,             // target processor
    ver_frontend_major: u16,  // front end major version #
    ver_frontend_eminor: u16, // front end minor version #
    ver_frontend_build: u16,  // front end build version #
    ver_major: u16,           // back end major version #
    ver_minor: u16,           // back end minor version #
    ver_build: u16,           // back end build version #
    version: NullString,      // Zero terminated compiler version string
}

impl RecordEntry for CompileSym1 {
    const RECORD_TYPE: u16 = 0x1116;
}

#[repr(C)]
#[derive(Debug)]
pub enum CompileSym {
    Symbol1(CompileSym1),
    Symbol3(CompileSym3),
}

impl RecordEntry for CompileSym {
    fn is_valid_record_type(record_type: u16) -> bool {
        CompileSym1::is_valid_record_type(record_type)
            || CompileSym3::is_valid_record_type(record_type)
    }
}

impl binrw::BinRead for CompileSym {
    type Args<'a> = ();

    fn read_options<R: std::io::Read + std::io::Seek>(
        reader: &mut R,
        endian: binrw::Endian,
        args: Self::Args<'_>,
    ) -> binrw::BinResult<Self> {
        let start = reader.stream_position()?;
        reader.seek_relative(2)?;
        let record_type = <u16>::read_options(reader, endian, args)?;
        reader.seek(std::io::SeekFrom::Start(start))?;
        Ok(if CompileSym1::is_valid_record_type(record_type) {
            Self::Symbol1(CompileSym1::read_options(reader, endian, args)?)
        } else if CompileSym3::is_valid_record_type(record_type) {
            Self::Symbol3(CompileSym3::read_options(reader, endian, args)?)
        } else {
            return Err(binrw::Error::NoVariantMatch { pos: start + 2 });
        })
    }
}

#[repr(C)]
#[derive(Debug, binrw::BinRead)]
pub struct Namespace {
    reclen: u16, // Record length
    rectyp: u16, // S_COMPILE2
    name: NullString,
}

impl RecordEntry for Namespace {
    const RECORD_TYPE: u16 = 0x1124;
}

#[repr(C)]
#[derive(Debug, binrw::BinRead)]
pub struct ConstantSymbol {
    reclen: u16, // Record length
    rectyp: u16, // S_CONSTANT or S_MANCONSTANT
    type_index: TypeId,
    value: u16, //
    name: CodeViewString,
}

impl RecordEntry for ConstantSymbol {
    const RECORD_TYPE: u16 = 0x1107;
}

// enum RecordEntries {
//     CompileSym(CompileSym),
//     Namespace(Namespace),
//     ConstantSymbol(ConstantSymbol),
// }
