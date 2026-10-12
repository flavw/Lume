use std::cell::Cell;

#[derive(Clone, Copy, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Language {
    #[default]
    Portuguese,
    English,
}

thread_local! { static ENGLISH: Cell<bool> = const { Cell::new(false) }; }
pub fn set_language(language: Language) {
    ENGLISH.set(language == Language::English);
}

pub fn t(text: &str) -> String {
    let text = text.trim_end_matches("...").trim_end_matches('…');
    if !ENGLISH.get() {
        return text.to_owned();
    }
    let pairs = [
        ("Formato", "Format"),
        ("Salvar imagem", "Save image"),
        ("PNG preserva a imagem sem perdas; não usa qualidade percentual.", "PNG preserves the image losslessly; percentage quality does not apply."),

        ("Atalhos", "Shortcuts"),
        ("Fim da pasta", "End of folder"),
        ("Você chegou à última imagem desta pasta.", "You have reached the last image in this folder."),
        ("Deseja voltar à primeira imagem ou abrir outra imagem?", "Return to the first image or open another image?"),
        ("Reiniciar", "Restart"),
        ("Abrir outra imagem", "Open another image"),
        ("Reiniciar automaticamente ao chegar ao fim da pasta", "Automatically restart at the end of the folder"),
        ("Tab ou setas: escolher · Enter: confirmar · Esc: cancelar", "Tab or arrows: select · Enter: confirm · Esc: cancel"),

        ("Criado em", "Created"), ("Data de criação do arquivo no sistema", "File creation time on the filesystem"),
        ("Conteúdo de IA copiado.", "AI content copied."),

        ("Barra de ferramentas", "Toolbar"), ("Barra de status", "Status bar"),
        ("Anterior", "Previous"), ("Próxima", "Next"), ("Ajustar", "Fit to window"),
        ("Nenhuma imagem aberta", "No image open"), ("Modificada", "Modified"),

        ("Cancelar", "Cancel"), ("Fechar", "Close"),
        ("Este arquivo será excluído permanentemente.", "This file will be permanently deleted."),
        ("Você poderá restaurar o arquivo pela lixeira.", "You can restore the file from the recycle bin."),
        ("Enter: confirmar opção · Tab: alternar · Esc: cancelar", "Enter: confirm choice · Tab: switch · Esc: cancel"),
        ("Copiar tudo", "Copy all"), ("Metadados copiados.", "Metadata copied."),
        ("Geração por IA · parameters", "AI generation · parameters"),
        ("Geração por IA · prompt", "AI generation · prompt"),
        ("Geração por IA · workflow", "AI generation · workflow"),

        ("Exportar WebP", "Export WebP"),
        ("Salvar no formato original…", "Save in original format…"), ("Salvar no formato original", "Save in original format"),
        ("Informações da imagem…", "Image information…"), ("Informações da imagem", "Image information"),
        ("Substituir o arquivo atual pelas alterações?", "Replace the current file with your changes?"),
        ("O formato real será mantido. Formatos com perdas serão recomprimidos e os metadados serão removidos.", "The actual format will be retained. Lossy formats will be recompressed and metadata will be removed."),
        ("Salvar", "Save"), ("Imagem salva.", "Image saved."),
        ("Fundo quadriculado para transparência", "Checkerboard background for transparency"),
        ("Dimensões", "Dimensions"), ("Transparência", "Transparency"), ("Arquivo", "File"),
        ("Tamanho", "Size"), ("Formato real", "Actual format"), ("Origem", "Source"),
        ("Documento em memória", "In-memory document"), ("Sim", "Yes"), ("Não", "No"),

        ("Mostrar título da arte (T, somente em tela cheia)", "Show artwork title (T, fullscreen only)"),
        ("Ocultar cursor em tela cheia (F11)", "Hide mouse cursor in fullscreen (F11)"),
        ("Intervalo da apresentação", "Slideshow interval"),
        ("Modo de zoom (1–7)", "Zoom mode (1–7)"),

        ("Tela cheia / Apresentação", "Fullscreen / Slideshow"),
        ("Exportação", "Export"), ("Cor da tela", "Screen color"),
        ("Escolher…", "Choose…"), ("Tamanho do texto", "Text size"),
        ("Posição do texto", "Text position"), ("Alinhamento do texto", "Text alignment"),
        ("Topo", "Top"), ("Meio", "Middle"), ("Embaixo", "Bottom"),
        ("Esquerda", "Left"), ("Centro", "Center"), ("Direita", "Right"),
        ("Mostrar título da arte (somente em tela cheia)", "Show artwork title (fullscreen only)"),
        ("Ocultar cursor em tela cheia", "Hide mouse cursor in fullscreen"),

        ("Selecionar imagem inteira", "Select entire image"),
        ("Sobre", "About"),
        ("Sobre o LumeView", "About LumeView"),
        ("Um visualizador de imagens simples e rápido.", "A simple, fast image viewer."),
        ("Criado por flavw · CC0 1.0", "Created by flavw · CC0 1.0"),
        ("Software open source · CC0 1.0", "Open source software · CC0 1.0"),

        ("Cor do título em tela cheia", "Fullscreen title color"),
        ("Mostrar / ocultar título da arte (tela cheia)", "Show / hide artwork filename (fullscreen)"),
        ("Tamanho original", "Original size"),
        ("Selecionar / desselecionar tudo", "Select / deselect all"),
        ("Imagem anterior / próxima; com seleção, mover recorte", "Previous / next image; with a selection, move it"),
        ("Mover imagem verticalmente; com seleção, mover recorte", "Pan vertically; with a selection, move it"),
        ("Próxima / anterior", "Next / previous"),
        ("Pular 5 imagens", "Jump 5 images"),
        ("Avançar N (0 = 10); Shift inverte", "Advance N images (0 = 10); Shift reverses"),
        ("Primeira / última imagem", "First / last image"),
        ("Ampliar / reduzir", "Zoom in / out"),
        ("Modos de ajuste, conforme menu Exibir", "Fit modes listed in the View menu"),
        ("Cancelar seleção, sair da tela cheia ou fechar", "Cancel selection, exit fullscreen or close"),
        ("Renomear, incluindo extensão", "Rename, including extension"),
        ("Mover / copiar arquivo", "Move / copy file"),
        ("Lixeira / exclusão permanente, com confirmação", "Recycle bin / permanent deletion, with confirmation"),
        ("Ocultar / mostrar cursor em tela cheia", "Hide / show cursor in fullscreen"),
        ("Exportar JPEG / WebP", "Export JPEG / WebP"),
        ("Aplicar recorte selecionado", "Apply selection"),
        ("Iniciar recorte JPEG sem perdas", "Start lossless JPEG selection"),
        ("Redimensionar seleção proporcionalmente", "Resize selection proportionally"),
        ("Mover seleção em passos maiores", "Move selection in larger steps"),
        ("Alterar largura / altura da seleção", "Change selection width / height"),
        ("Desfazer / refazer edições", "Undo / redo edits"),
        ("Copiar imagem ou seleção", "Copy image or selection"),
        ("Procurar na pasta", "Find in folder"),
        ("Abrir em outra janela", "Open in another window"),
        ("Iniciar / parar apresentação", "Start / stop slideshow"),
        ("Pausar apresentação; outra tecla continua", "Pause slideshow; another key resumes"),
        ("Espaço / Backspace", "Space / Backspace"),
        ("Ctrl + navegação", "Ctrl + navigation"),
        ("LumeView 0.7 · Visualizador minimalista em Rust", "LumeView 0.7 · Minimal image viewer in Rust"),
        ("Edições comuns ficam em memória até exportar. Navegar descarta essas ediçōes.", "Edits remain in memory until exported. Navigation discards them."),
        ("Edições comuns ficam em memória até exportar. Navegar descarta essas edições. Recorte JPEG sem perdas salva uma cópia com sufixo _crop, _crop1, _crop2, preservando o original. JPEG com orientação EXIF diferente de 1 usa recorte comum nesta versão.", "Edits remain in memory until exported. Navigation discards them. Lossless JPEG selections save a separate _crop, _crop1, _crop2 file and preserve the original. JPEGs with EXIF orientation other than 1 require a regular selection in this version."),
        ("Imagens animadas e multipáginas: apenas o primeiro quadro. Sem OCR, miniaturas, impressão, captura de tela, filtros 8BF ou edição IPTC nesta versão.", "Animated and multipage images: first frame only. OCR, thumbnails, printing, screen capture, 8BF filters and IPTC editing are not available in this version."),
        ("JPEG/WebP são abertos diretamente. ImageMagick atende formatos adicionais e exportação sob demanda. jpegtran realiza recorte JPEG sem recompressão.", "JPEG/WebP open directly. ImageMagick handles additional formats and export on demand. jpegtran saves JPEG selections without recompression."),
        ("Linux: instale ImageMagick 7 com suporte AVIF/WebP e libjpeg-turbo. Variáveis LUMEVIEW_MAGICK e LUMEVIEW_JPEGTRAN permitem indicar os executáveis.", "Linux: install ImageMagick 7 with AVIF/WebP support and libjpeg-turbo. LUMEVIEW_MAGICK and LUMEVIEW_JPEGTRAN specify alternative executable paths."),
        ("Converte os arquivos originais. Nomes existentes recebem um sufixo numérico.", "Converts original files. Existing names receive a numeric suffix."),
        ("Excluir permanentemente este arquivo?", "Permanently delete this file?"),
        ("Enviar este arquivo à lixeira?", "Move this file to the recycle bin?"),
        ("Arquivo não encontrado.", "File not found."),
        ("Imagem copiada.", "Image copied."),
        ("Exportação concluída.", "Export completed."),
        ("Lote concluído.", "Batch completed."),
        ("Lote cancelado. Arquivos concluídos foram mantidos.", "Batch canceled. Completed files were kept."),
        ("Recorte: Ctrl+Y aplica em memória; Esc cancela.", "Selection: Ctrl+Y applies in memory; Esc cancels."),
        ("Recorte JPEG sem perdas: Ctrl+Y salva uma cópia na mesma pasta; Esc cancela.", "Lossless JPEG selection: Ctrl+Y saves a copy in the same folder; Esc cancels."),
        ("Esta versão exige JPEG com orientação EXIF normal (1) para crop sem perdas. Use o recorte comum.", "Lossless JPEG selections require normal EXIF orientation (1). Use a regular selection."),
        ("O recorte JPEG sem perdas exige a imagem original, sem edições em memória. Recarregue com F5.", "Lossless JPEG selections require the unedited original. Reload with F5."),
        ("O recorte sem perdas está disponível apenas para arquivos JPEG.", "Lossless selections are available only for JPEG files."),
        ("Limite desta versão: 100 megapixels.", "Limit in this version: 100 megapixels."),
        ("Arquivo renomeado. O conteúdo e o formato não foram convertidos.", "File renamed. Its content and format were not converted."),
        ("Já existe um arquivo com esse nome. Nenhum arquivo foi substituído.", "A file with this name already exists. No file was replaced."),
        ("O destino já existe. Nenhum arquivo foi substituído.", "The destination already exists. No file was replaced."),
        ("Copiar para pasta", "Copy to folder"),
        ("Mover para pasta", "Move to folder"),
        ("imagens selecionadas", "images selected"),
        ("proporção", "aspect ratio"),
        ("Cancelar após a imagem atual", "Cancel after the current image"),
        ("Português", "Português"),
        ("English", "English"),
        ("1  Tamanho original (1:1)", "1  Original size (1:1)"),
        (
            "2  Ajustar apenas imagens grandes",
            "2  Fit large images only",
        ),
        ("3  Ajustar todas à janela", "3  Fit all images to window"),
        ("4  Esticar à janela", "4  Stretch to window"),
        ("5  Ajustar largura", "5  Fit width"),
        ("6  Ajustar altura", "6  Fit height"),
        ("7  Ajustar lado menor", "7  Fit shorter side"),
        (
            "Preferências são salvas ao fechar o aplicativo.",
            "Preferences are saved when the app closes.",
        ),
        (
            "Nome completo, incluindo a extensão:",
            "Full filename, including extension:",
        ),
        (
            "Alterar a extensão não converte o formato da imagem.",
            "Changing the extension does not convert the image format.",
        ),
        ("Recorte JPEG sem perdas", "Lossless JPEG selection"),
        (
            "Recortar (última proporção)",
            "Select area (last aspect ratio)",
        ),
        ("Aplicar recorte", "Apply selection"),
        ("Cancelar seleção", "Cancel selection"),
        ("Inverter proporção", "Invert aspect ratio"),
        ("Recarregar imagem e pasta", "Reload image and folder"),
        ("Procurar na pasta", "Find in folder"),
        ("Imagem aleatória", "Random image"),
        ("Iniciar apresentação", "Start slideshow"),
        ("Parar apresentação", "Stop slideshow"),
        ("Sair da tela cheia", "Exit fullscreen"),
        ("Tela cheia / janela", "Fullscreen / window"),
        ("Imagem anterior", "Previous image"),
        ("Próxima imagem", "Next image"),
        ("Modo de zoom", "Zoom mode"),
        ("Fechar programa", "Close app"),
        ("Espelhar horizontalmente", "Flip horizontally"),
        ("Espelhar verticalmente", "Flip vertically"),
        ("Girar 90° à direita", "Rotate 90° clockwise"),
        ("Girar 90° à esquerda", "Rotate 90° counterclockwise"),
        ("Escala de cinza", "Grayscale"),
        ("Redimensionar", "Resize"),
        ("Copiar imagem / seleção", "Copy image / selection"),
        ("Colar como nova imagem", "Paste as new image"),
        ("Exportar", "Export"),
        ("Qld", "Qlt"),
        ("sem perdas", "lossless"),
        ("Converter em lote", "Batch conversion"),
        ("Mover arquivo", "Move file"),
        ("Copiar arquivo", "Copy file"),
        ("Enviar à lixeira", "Move to recycle bin"),
        ("Excluir permanentemente", "Delete permanently"),
        ("Abrir imagem", "Open image"),
        ("Renomear", "Rename"),
        ("Abrir", "Open"),
        ("Desfazer", "Undo"),
        ("Refazer", "Redo"),
        ("Rotação JPEG sem perdas", "Lossless JPEG rotation"),
        ("Girar 180°", "Rotate 180°"),
        ("Recorte JPEG", "JPEG crop"),
        ("Transparência dos menus", "Menu transparency"),
        ("Alterações não salvas", "Unsaved changes"),
        ("Esta imagem tem alterações não salvas. Deseja descartá-las?", "This image has unsaved changes. Discard them?"),
        ("Descartar alterações", "Discard changes"),
        ("Novo nome:", "New name:"),
        ("Extensão:", "Extension:"),
        ("Neon + preto", "Neon + black"),
        ("Proporção manual", "Custom aspect ratio"),
        ("Proporção largura : altura", "Width : height ratio"),
        ("Inverter", "Swap"),
        ("Cor neon", "Neon color"),
        ("Imprimir", "Print"),
        ("Recortar", "Select area"),
        ("Recortar sem perdas", "Lossless crop"),
        ("Copiar", "Copy"),
        ("Colar", "Paste"),
        ("Arquivo", "File"),
        ("Editar", "Edit"),
        ("Imagem", "Image"),
        ("Opções", "Options"),
        ("Exibir", "View"),
        ("Ajuda", "Help"),
        ("Preferências", "Preferences"),
        ("Aparência", "Appearance"),
        ("Tema", "Theme"),
        ("Idioma", "Language"),
        ("Escuro", "Dark"),
        ("Claro", "Light"),
        ("Sistema", "System"),
        ("Qualidade de exportação", "Export quality"),
        (
            "JPEG e WebP • qualidade padrão",
            "JPEG and WebP • default quality",
        ),
        ("Visualização", "Viewing"),
        ("Apresentação", "Slideshow"),
        ("Intervalo entre imagens", "Time between images"),
        ("segundos", "seconds"),
        ("Modo inicial", "Startup mode"),
        ("Atalhos", "Shortcuts"),
        ("Dependências", "Dependencies"),
        ("Fechar / cancelar", "Close / cancel"),
        ("Largura", "Width"),
        ("Altura", "Height"),
        ("Manter proporção", "Keep aspect ratio"),
        ("Aplicar", "Apply"),
        ("Substituir", "Replace"),
        ("Aviso", "Notice"),
        ("Selecionar imagens", "Select images"),
        (
            "Usar imagens da pasta atual",
            "Use images from current folder",
        ),
        ("Escolher pasta de saída", "Choose output folder"),
        ("Qualidade", "Quality"),
        ("Destino", "Destination"),
        ("Converter", "Convert"),
        ("Parte do nome do arquivo", "Part of filename"),
        ("Livre", "Free"),
        ("Continuar", "Resume"),
        ("Pausar", "Pause"),
        ("Sair", "Exit"),
        ("Ampliar", "Zoom in"),
        ("Reduzir", "Zoom out"),
        ("Processando", "Processing"),
        ("Convertendo", "Converting"),
        ("Zoom manual", "Manual zoom"),
        ("Recorte salvo", "Selection saved"),
        ("Imagem copiada.", "Image copied."),
        ("Novo documento", "Untitled image"),
        (
            "Abra uma imagem ou cole da área de transferência.",
            "Open an image or paste from the clipboard.",
        ),
        (
            "Arraste um arquivo para a janela • Ctrl+O abrir • Ctrl+V colar",
            "Drop a file here • Ctrl+O open • Ctrl+V paste",
        ),
    ];
    if let Some((_, english)) = pairs
        .iter()
        .find(|(pt, _)| pt.trim_end_matches('…') == text)
    {
        return english.trim_end_matches('…').to_owned();
    }
    if let Some(label) = text.strip_suffix('…') {
        return format!("{}…", t(label));
    }
    if let Some(rest) = text.strip_prefix("Exportar ") {
        return format!(
            "Export {}",
            rest.replace("Qld", "Qlt").replace("sem perdas", "lossless")
        );
    }
    if let Some(rest) = text.strip_prefix("Recorte salvo: ") {
        return format!("Selection saved: {rest}");
    }
    if let Some(rest) = text.strip_prefix("Zoom manual") {
        return format!("Manual zoom{rest}");
    }
    if text.contains(" · JPEG sem perdas · proporção ") {
        return text.replace(
            " · JPEG sem perdas · proporção ",
            " · lossless JPEG · aspect ratio ",
        );
    }
    text.to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn language_switch_and_quality_labels() {
        set_language(Language::English);
        assert_eq!(t("Exportar WebP (Qld 80%)…"), "Export WebP (Qlt 80%)");
        assert_eq!(t("5  Ajustar largura"), "5  Fit width");
        set_language(Language::Portuguese);
        assert_eq!(t("Arquivo"), "Arquivo");
    }
}
