use pdb2::{AddressMap, DebugInformation, FallibleIterator, ModuleInfo, SymbolData, PDB};
use std::{borrow::Cow, fs::File};
use windows::Win32::System::{
    Diagnostics::Debug::{
        IMAGE_DATA_DIRECTORY, IMAGE_DEBUG_DIRECTORY, IMAGE_DEBUG_TYPE_CODEVIEW,
        IMAGE_DIRECTORY_ENTRY, IMAGE_DIRECTORY_ENTRY_DEBUG, IMAGE_DIRECTORY_ENTRY_EXPORT,
        IMAGE_NT_HEADERS64,
    },
    SystemInformation::IMAGE_FILE_MACHINE_AMD64,
    SystemServices::{IMAGE_DOS_HEADER, IMAGE_EXPORT_DIRECTORY},
};

use crate::{error::Error, memory::MemorySource};

enum AddressMatch<'a> {
    None,
    Export(&'a Export),
    Public(String),
}
impl AddressMatch<'_> {
    fn is_none(&self) -> bool {
        matches!(self, AddressMatch::None)
    }

    fn to_symbol_name(&self) -> Option<String> {
        Some(match self {
            AddressMatch::None => return None,
            AddressMatch::Export(e) => e
                .name
                .clone()
                .unwrap_or_else(|| format!("Ordinal{}", e.ordinal)),
            AddressMatch::Public(it) => it.clone(),
        })
    }
}

#[derive(Debug, Default)]
pub struct Process {
    modules: Vec<Module>,
    threads: Vec<u32>,
}

impl Process {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_module<M: MemorySource>(
        &mut self,
        address: u64,
        name: Option<String>,
        memory: M,
    ) -> Result<&Module, Error> {
        let module = Module::from_memory_view(address, name, memory)?;
        self.modules.push(module);
        Ok(self.modules.last().unwrap())
    }

    pub fn add_thread(&mut self, thread_id: u32) {
        self.threads.push(thread_id);
    }

    pub fn remove_thread(&mut self, thread_id: u32) {
        self.threads.retain(|x| *x != thread_id);
    }

    pub fn threads(&self) -> &[u32] {
        &self.threads
    }

    pub fn name_to_address(
        &mut self,
        module_name: &str,
        function_name: &str,
    ) -> Result<u64, Error> {
        self.get_module_by_name_mut(module_name)
            .ok_or_else(|| Error::UnknownModuleName(module_name.into()))?
            .resolve_function(function_name)
            .ok_or(Error::Todo)
    }

    pub fn address_to_name(&mut self, address: u64) -> Option<String> {
        let module = self.get_module_by_address_mut(address)?;
        let mut closest: AddressMatch = AddressMatch::None;
        let mut closest_addr: u64 = 0;
        // This could be faster if we were always in sorted order
        if let Some(export) = module
            .exports
            .iter()
            .find(|e| e.target.as_rva().is_some_and(|a| a <= address))
        {
            if closest.is_none() {
                closest = AddressMatch::Export(export);
                closest_addr = export.target.as_rva().unwrap();
            }
        }

        if let Some((symbol_table, address_map)) = module
            .pdb
            .as_mut()
            .and_then(|p| Some((p.global_symbols().ok()?, p.address_map().ok()?)))
        {
            let mut symbols = symbol_table.iter();
            while let Ok(Some(symbol)) = symbols.next() {
                match symbol.parse() {
                    Ok(pdb2::SymbolData::Public(data)) if data.function => {
                        let rva = data.offset.to_rva(&address_map).unwrap_or_default();
                        let global_addr = module.address + rva.0 as u64;
                        if global_addr <= address
                            && (closest.is_none() || closest_addr <= global_addr)
                        {
                            // TODO: Take a reference to the data?
                            closest = AddressMatch::Public(data.name.to_string().to_string());
                            closest_addr = global_addr;
                        }
                    }
                    _ => {}
                }
            }
        }

        let symbol_name = closest.to_symbol_name()?;
        let offset = address - closest_addr;
        Some(if offset == 0 {
            format!("{}!{}", &module.name(), symbol_name)
        } else {
            format!("{}!{}+0x{:X}", &module.name(), symbol_name, offset)
        })
    }

    pub(crate) fn get_module_by_address_mut(&mut self, address: u64) -> Option<&mut Module> {
        self.modules
            .iter_mut()
            .find(|m| m.contains_address(address))
    }

    pub(super) fn get_module_by_name(&self, module_name: &str) -> Option<&Module> {
        self.modules
            .iter()
            .find(|m| name_equals(m.name(), module_name))
    }

    pub(super) fn get_module_by_name_mut(&mut self, module_name: &str) -> Option<&mut Module> {
        self.modules
            .iter_mut()
            .find(|m| name_equals(m.name(), module_name))
    }

    pub(crate) fn module_names(&self) -> Vec<String> {
        self.modules.iter().map(|m| m.name().into_owned()).collect()
    }

    pub(crate) fn get_module_by_address(&self, address: u64) -> Option<&Module> {
        self.modules.iter().find(|m| m.contains_address(address))
    }
}

fn name_equals(module_name: Cow<str>, needle_name: &str) -> bool {
    let module_name = module_name.to_lowercase();
    let module_name = &module_name;
    let needle_name = needle_name.to_lowercase();
    module_name == &needle_name
        || module_name
            .split('\\')
            .last()
            .as_ref()
            .is_some_and(|m| m == &needle_name)
}

#[derive(Default)]
struct ModuleBuilder {
    pub name: Option<String>,
    pub address: u64,
    pub size: u64,
    pub exports: Vec<Export>,
    pub pdb_name: Option<String>,
    pub pdb_info: Option<PdbInfo>,
    pub pdb: Option<PDB<'static, File>>,
    pub address_map: Option<AddressMap<'static>>,
    pe_header: IMAGE_NT_HEADERS64,
}

impl ModuleBuilder {
    fn read_debug_info<M: MemorySource>(
        &mut self,
        pe_header: IMAGE_NT_HEADERS64,
        memory: &M,
    ) -> Result<(), Error> {
        let debug_table_info =
            pe_header.OptionalHeader.DataDirectory[IMAGE_DIRECTORY_ENTRY_DEBUG.0 as usize];
        if debug_table_info.VirtualAddress == 0 {
            return Ok(());
        }
        let dir_size = std::mem::size_of::<IMAGE_DEBUG_DIRECTORY>() as u64;
        // We'll arbitrarily limit to 20 entries to keep it sane.
        let count: u64 = (debug_table_info.Size as u64 / dir_size).min(20);
        for dir_index in 0..count {
            let debug_directory_address =
                self.address + (debug_table_info.VirtualAddress as u64) + (dir_index * dir_size);
            let debug_directory: IMAGE_DEBUG_DIRECTORY =
                memory.read_memory_data(debug_directory_address)?;
            if debug_directory.Type == IMAGE_DEBUG_TYPE_CODEVIEW {
                let pdb_info_address = debug_directory.AddressOfRawData as u64 + self.address;
                self.pdb_info = Some(memory.read_memory_data(pdb_info_address)?);
                // We could check that pdb_info.signature is RSDS here.
                let pdb_name_address = pdb_info_address + std::mem::size_of::<PdbInfo>() as u64;
                let max_size = debug_directory.SizeOfData as usize - std::mem::size_of::<PdbInfo>();
                self.pdb_name =
                    Some(memory.read_memory_string(pdb_name_address, max_size, false)?);

                let pdb_file = File::open(self.pdb_name.as_ref().unwrap());
                if let Ok(pdb_file) = pdb_file {
                    let pdb_data = PDB::open(pdb_file);
                    if let Ok(pdb_data) = pdb_data {
                        self.pdb = Some(pdb_data);
                        self.address_map = self.pdb.as_mut().and_then(|pdb| pdb.address_map().ok());
                    }
                }
            }
        }
        Ok(())
    }

    fn read_exports<M: MemorySource>(
        &mut self,
        pe_header: IMAGE_NT_HEADERS64,
        memory: &M,
    ) -> Result<(), Error> {
        // let mut module_name: Option<String> = None;
        let export_table_info =
            pe_header.OptionalHeader.DataDirectory[IMAGE_DIRECTORY_ENTRY_EXPORT.0 as usize];
        if export_table_info.VirtualAddress != 0 {
            let export_table_addr = self.address + export_table_info.VirtualAddress as u64;
            let export_table_end = export_table_addr + export_table_info.Size as u64;
            let export_directory: IMAGE_EXPORT_DIRECTORY =
                memory.read_memory_data(export_table_addr)?;

            // This is a fallback that lets us find a name if none was available.
            if export_directory.Name != 0 && self.name.is_none() {
                let name_addr = self.address + export_directory.Name as u64;
                self.name = Some(memory.read_memory_string(name_addr, 512, false)?);
            }

            // We'll read the name table first, which is essentially a list of (ordinal, name) pairs that give names
            // to some or all of the exports. The table is stored as parallel arrays of orindals and name pointers
            let ordinal_array_address =
                self.address + export_directory.AddressOfNameOrdinals as u64;
            let ordinal_array = memory.read_memory_full_array::<u16>(
                ordinal_array_address,
                export_directory.NumberOfNames as usize,
            )?;
            let name_array_address = self.address + export_directory.AddressOfNames as u64;
            let name_array = memory.read_memory_full_array::<u32>(
                name_array_address,
                export_directory.NumberOfNames as usize,
            )?;

            let address_table_address = self.address + export_directory.AddressOfFunctions as u64;
            let address_table = memory.read_memory_full_array::<u32>(
                address_table_address,
                export_directory.NumberOfFunctions as usize,
            )?;

            for (unbiased_ordinal, function_address) in address_table.iter().enumerate() {
                let ordinal = export_directory.Base + unbiased_ordinal as u32;
                let target_address = self.address + *function_address as u64;

                let name_index = ordinal_array
                    .iter()
                    .position(|&o| o == unbiased_ordinal as u16);
                let export_name = match name_index {
                    None => None,
                    Some(idx) => {
                        let name_address = self.address + name_array[idx] as u64;
                        Some(memory.read_memory_string(name_address, 4096, false)?)
                    }
                };

                // An address that falls inside the export directory is actually a forwarder
                let export = if target_address >= export_table_addr
                    && target_address < export_table_end
                {
                    // I don't know that there actually is a max size for a forwader name, but 4K is probably reasonable.
                    let forwarding_name = memory.read_memory_string(target_address, 4096, false)?;
                    Export {
                        name: export_name,
                        ordinal,
                        target: ExportTarget::Forwarder(forwarding_name),
                    }
                } else {
                    Export {
                        name: export_name,
                        ordinal,
                        target: ExportTarget::Rva(target_address),
                    }
                };
                self.exports.push(export);
            }
        };

        Ok(())
    }

    fn build(mut self) -> Result<Module, Error> {
        let Some(pdb) = self.pdb.as_mut() else {
            return Ok(Module {
                name: self.name,
                address: self.address,
                size: self.size,
                exports: self.exports,
                pdb_name: self.pdb_name,
                pdb_info: self.pdb_info,
                pdb: self.pdb,
                address_map: self.address_map,
                pe_header: self.pe_header,
                debug_information: None,
                module_informations: Vec::new(),
            });
        };
        let debug_information = pdb.debug_information()?;
        let module_informations: Result<Result<Vec<_>, _>, _> = debug_information
            .modules()?
            .iterator()
            .map(|m| m.map(|m| pdb.module_info(&m)))
            .collect();
        let module_informations = module_informations??;
        let module_informations: Vec<_> = module_informations.into_iter().flatten().collect();
        Ok(Module {
            name: self.name,
            address: self.address,
            size: self.size,
            exports: self.exports,
            pdb_name: self.pdb_name,
            pdb_info: self.pdb_info,
            pdb: self.pdb,
            address_map: self.address_map,
            pe_header: self.pe_header,
            debug_information: Some(debug_information),
            module_informations,
        })
    }
}

pub struct Module {
    pub name: Option<String>,
    pub address: u64,
    pub size: u64,
    pub exports: Vec<Export>,
    pub pdb_name: Option<String>,
    pub pdb_info: Option<PdbInfo>,
    pub pdb: Option<PDB<'static, File>>,
    pub address_map: Option<AddressMap<'static>>,
    pub debug_information: Option<DebugInformation<'static>>,
    pub module_informations: Vec<ModuleInfo<'static>>,
    pe_header: IMAGE_NT_HEADERS64,
}

impl std::fmt::Debug for Module {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Module")
            .field("name", &self.name)
            .field("address", &self.address)
            .field("size", &self.size)
            .field("exports", &self.exports)
            .field("pdb_name", &self.pdb_name)
            .field("pdb_info", &self.pdb_info)
            .field("pdb", &self.pdb)
            .field("address_map", &self.address_map)
            .field("debug_information", &self.debug_information)
            // .field("module_informations", &self.module_informations)
            .finish()
    }
}

impl Module {
    pub fn name(&self) -> Cow<str> {
        self.name
            .as_ref()
            .map(|s| s.into())
            .unwrap_or_else(|| format!("module_{:X}", self.address).into())
    }

    fn from_memory_view<M: MemorySource>(
        address: u64,
        name: Option<String>,
        memory: M,
    ) -> Result<Self, Error> {
        let dos_header: IMAGE_DOS_HEADER = memory.read_memory_data(address)?;

        // NOTE: Do we trust that the headers are accurate, even if it means we could read outside the bounds of the
        //       module? For this debugger, we'll trust the data, but a real debugger should do sanity checks and
        //       report discrepancies to the user in some way.
        let pe_header_addr = address + dos_header.e_lfanew as u64;

        // NOTE: This should be IMAGE_NT_HEADERS32 for 32-bit modules, but the FileHeader lines up for both structures.
        let pe_header: IMAGE_NT_HEADERS64 = memory.read_memory_data(pe_header_addr)?;
        let size = pe_header.OptionalHeader.SizeOfImage as u64;

        if pe_header.FileHeader.Machine != IMAGE_FILE_MACHINE_AMD64 {
            todo!("Throw error!");
            // return Err("Unsupported machine architecture for module");
        }

        let mut result = ModuleBuilder {
            name,
            address,
            size,
            pe_header,
            ..Default::default()
        };

        result.read_debug_info(pe_header, &memory)?;
        result.read_exports(pe_header, &memory)?;

        result.build()
    }

    fn contains_address(&self, address: u64) -> bool {
        let end = self.address + self.size;
        self.address <= address && address < end
    }

    pub(super) fn resolve_function(&self, function_name: &str) -> Option<u64> {
        self.exports
            .iter()
            .find(|e| e.name.as_ref().is_some_and(|e| e == function_name))
            .and_then(|e| e.target.as_rva())
            .or_else(|| self.resolve_symbol(function_name))
    }

    fn resolve_symbol(&self, function_name: &str) -> Option<u64> {
        let address_map = self.address_map.as_ref()?;
        for pdb_module in &self.module_informations {
            let mut symbols = pdb_module.symbols().ok()?;
            while let Some(sym) = symbols.next().ok()? {
                if let Ok(SymbolData::Procedure(proc_data)) = sym.parse() {
                    if proc_data.name.to_string() == function_name {
                        let rva = proc_data.offset.to_rva(address_map)?;
                        let address = self.address + rva.0 as u64;
                        return Some(address);
                    }
                }
            }
        }
        // }
        None
    }

    pub(crate) fn get_data_directory(
        &self,
        entry: IMAGE_DIRECTORY_ENTRY,
    ) -> Option<IMAGE_DATA_DIRECTORY> {
        let result = self.pe_header.OptionalHeader.DataDirectory[entry.0 as usize];
        if result.Size == 0 || result.VirtualAddress == 0 {
            None
        } else {
            Some(result)
        }
    }
}

#[derive(Debug)]
pub struct Export {
    pub name: Option<String>,
    // This is the "biased" ordinal
    pub ordinal: u32,
    pub target: ExportTarget,
}
#[derive(Debug)]

pub enum ExportTarget {
    Rva(u64),
    Forwarder(String),
}
impl ExportTarget {
    fn as_rva(&self) -> Option<u64> {
        match self {
            ExportTarget::Rva(it) => Some(*it),
            _ => None,
        }
    }
}

#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
pub struct PdbInfo {
    pub signature: u32,
    pub guid: windows::core::GUID,
    pub age: u32,
    // Null terminated name goes after the end
}
