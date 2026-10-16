use eframe::egui::{Key, Modifiers};
use std::collections::BTreeMap;

// Navigation, text clipboard, Escape and Enter remain standard system keys.
pub const ACTIONS: &[(&str,&str)] = &[
    ("Abrir", "Ctrl+O"), ("Salvar imagem","Ctrl+S"), ("Exportar JPEG","S"),
    ("Exportar WebP","W"), ("Exportar PNG","Q"), ("Imprimir","Ctrl+P"),
    ("Informações da imagem","I"), ("Preferências","P"), ("Renomear","F2"),
    ("Recarregar imagem e pasta","F5"), ("Mover arquivo","F7"), ("Copiar arquivo","F8"),
    ("Tela cheia / janela","F"), ("Desfazer","Ctrl+Z"), ("Refazer","Ctrl+J"),
    ("Aplicar recorte","Ctrl+Y"), ("Redimensionar","Ctrl+R"), ("Escala de cinza","Ctrl+G"),
    ("Procurar na pasta","Ctrl+F"), ("Imagem aleatória","Ctrl+M"), ("Apresentação","Ctrl+W"),
    ("Atalhos","F1"), ("Alternar zoom entre centro e canto superior esquerdo","C"),
];
pub fn parse(value: &str) -> Option<(Key,Modifiers)> {
    let mut m = Modifiers::NONE;
    let mut key = None;
    for part in value.split('+').map(str::trim) {
        match part.to_ascii_lowercase().as_str() {
            "ctrl" => {m.ctrl=true;m.command=true;}, "alt" => m.alt=true, "shift" => m.shift=true,
            _ => { if key.is_some() { return None; } key=Some(Key::from_name(part).or_else(||Key::from_name(&part.to_ascii_uppercase()))?); }
        }
    }
    Some((key?,m))
}
fn same(a: (Key,Modifiers),b: (Key,Modifiers)) -> bool {
    a.0==b.0 && a.1.ctrl==b.1.ctrl && a.1.alt==b.1.alt && a.1.shift==b.1.shift
}
pub fn validation(map: &BTreeMap<String,String>) -> bool {
    let mut assigned=Vec::new();
    for (_,default) in ACTIONS {
        let Some(binding)=parse(map.get(*default).map(String::as_str).unwrap_or(default)) else {return false;};
        if assigned.iter().any(|previous| same(*previous,binding)) {return false;}
        let key=binding.0;
        if matches!(key,Key::Escape|Key::Enter|Key::Tab|Key::ArrowLeft|Key::ArrowRight|Key::ArrowUp|Key::ArrowDown|Key::Space|Key::Backspace|Key::Delete|Key::Home|Key::End|Key::PageUp|Key::PageDown|Key::Num0|Key::Num1|Key::Num2|Key::Num3|Key::Num4|Key::Num5|Key::Num6|Key::Num7|Key::Num8|Key::Num9|Key::Plus|Key::Minus|Key::Equals|Key::F6|Key::F11)
            || same(binding,parse("Ctrl+C").unwrap()) || same(binding,parse("Ctrl+V").unwrap()) || same(binding,parse("Ctrl+X").unwrap())
            || matches!(key,Key::Copy|Key::Cut|Key::Paste|Key::Insert)
            || ["Ctrl+A","Ctrl+H","Ctrl+N","Ctrl+Shift+L","T","U"].iter().any(|s|same(binding,parse(s).unwrap())) {return false;}
        assigned.push(binding);
    }
    true
}
pub fn remap(key: Key,m: Modifiers,map: &BTreeMap<String,String>) -> Option<(Key,Modifiers)> {
    if !validation(map) {return Some((key,m));}
    for (_,default) in ACTIONS {
        let assigned=parse(map.get(*default).map(String::as_str).unwrap_or(default)).unwrap();
        if same((key,m),assigned) {return parse(default);}
    }
    if ACTIONS.iter().any(|(_,default)|same((key,m),parse(default).unwrap())) {None} else {Some((key,m))}
}
pub fn label(default: &str,map: &BTreeMap<String,String>) -> String {map.get(default).cloned().unwrap_or_else(||default.to_owned())}
thread_local! {static CURRENT: std::cell::RefCell<BTreeMap<String,String>> = const {std::cell::RefCell::new(BTreeMap::new())};}
pub fn set_current(map: &BTreeMap<String,String>) {CURRENT.with(|current| *current.borrow_mut()=if validation(map) {map.clone()} else {BTreeMap::new()});}
pub fn current_label(default: &str) -> String {
    CURRENT.with(|map| {
        let map=map.borrow();
        if let Some(value)=map.get(default) {return value.clone();}
        if default.contains(" / ") {return default.split(" / ").map(|part|label(part,&map)).collect::<Vec<_>>().join(" / ");}
        default.to_owned()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn duplicates_reserved_keys_and_remap() {
        let mut map=BTreeMap::new(); assert!(validation(&map));
        map.insert("I".into(),"Alt+K".into()); assert!(validation(&map));
        assert!(same(remap(Key::K,Modifiers::ALT,&map).unwrap(),parse("I").unwrap()));
        assert!(remap(Key::I,Modifiers::NONE,&map).is_none());
        map.insert("P".into(),"Alt+K".into()); assert!(!validation(&map));
        map.insert("P".into(),"Escape".into()); assert!(!validation(&map));
    }
    #[test]
    fn alignment_shortcut_can_be_remapped_without_affecting_copy() {
        let mut map=BTreeMap::new();
        assert!(same(remap(Key::C,Modifiers::NONE,&map).unwrap(),parse("C").unwrap()));
        map.insert("C".into(),"Alt+K".into());
        assert!(validation(&map));
        assert!(same(remap(Key::K,Modifiers::ALT,&map).unwrap(),parse("C").unwrap()));
        assert!(remap(Key::C,Modifiers::NONE,&map).is_none());
        let (key,modifiers)=parse("Ctrl+C").unwrap();
        assert!(same(remap(key,modifiers,&map).unwrap(),(key,modifiers)));
    }
}
