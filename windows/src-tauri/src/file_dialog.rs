use std::path::PathBuf;
use windows::{
    core::w,
    Win32::{
        Foundation::HWND,
        System::Com::{
            CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_INPROC_SERVER,
            COINIT_APARTMENTTHREADED,
        },
        UI::Shell::{
            FileOpenDialog, IFileOpenDialog, FOS_ALLOWMULTISELECT, FOS_DONTADDTORECENT,
            FOS_FILEMUSTEXIST, FOS_FORCEFILESYSTEM, FOS_PATHMUSTEXIST, SIGDN_FILESYSPATH,
        },
    },
};

struct Apartment;
impl Drop for Apartment {
    fn drop(&mut self) {
        unsafe {
            CoUninitialize();
        }
    }
}
pub fn choose(owner: isize) -> Result<Vec<String>, String> {
    unsafe {
        CoInitializeEx(None, COINIT_APARTMENTTHREADED)
            .ok()
            .map_err(|_| {
                "Não foi possível abrir o seletor de arquivos nesta thread.".to_string()
            })?;
        let _apartment = Apartment;
        let dialog: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER)
            .map_err(|_| "Não foi possível criar o seletor de documentos.".to_string())?;
        dialog
            .SetOptions(
                FOS_ALLOWMULTISELECT
                    | FOS_FORCEFILESYSTEM
                    | FOS_FILEMUSTEXIST
                    | FOS_PATHMUSTEXIST
                    | FOS_DONTADDTORECENT,
            )
            .map_err(|_| "Não foi possível configurar a seleção de documentos.".to_string())?;
        dialog
            .SetTitle(w!("Anexar documentos ao Coucou"))
            .map_err(|_| "Não foi possível configurar o título.".to_string())?;
        if let Err(error) = dialog.Show(Some(HWND(owner as *mut _))) {
            if error.code().0 == 0x800704c7u32 as i32 {
                return Ok(Vec::new());
            }
            return Err("Não foi possível concluir a seleção de documentos.".into());
        }
        let results = dialog
            .GetResults()
            .map_err(|_| "Não foi possível obter os documentos selecionados.".to_string())?;
        let count = results
            .GetCount()
            .map_err(|_| "Seleção de documentos inválida.".to_string())?;
        if count > crate::documents::MAX_FILES as u32 {
            return Err("Selecione no máximo 10 documentos por conversa.".into());
        }
        let mut paths = Vec::new();
        for index in 0..count {
            let item = results
                .GetItemAt(index)
                .map_err(|_| "Um documento não está disponível.".to_string())?;
            let path = item
                .GetDisplayName(SIGDN_FILESYSPATH)
                .map_err(|_| "Selecione arquivos locais.".to_string())?;
            let text = path
                .to_string()
                .map_err(|_| "O caminho de um documento é inválido.".to_string());
            CoTaskMemFree(Some(path.0.cast()));
            let path = PathBuf::from(text?);
            if !path.is_absolute() {
                return Err("Selecione um arquivo com caminho absoluto.".into());
            }
            paths.push(path.to_string_lossy().into_owned());
        }
        Ok(paths)
    }
}
