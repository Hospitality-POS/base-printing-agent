//! Label printers: render a device-independent label layout to ZPL (Zebra)
//! or TSPL (TSC / Xprinter / most cheap label printers).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PrinterLanguage {
    #[default]
    EscPos,
    Zpl,
    Tspl,
}

impl PrinterLanguage {
    pub fn parse(s: Option<&str>) -> Self {
        match s.unwrap_or("").trim().to_lowercase().as_str() {
            "zpl" => PrinterLanguage::Zpl,
            "tspl" | "tspl2" => PrinterLanguage::Tspl,
            _ => PrinterLanguage::EscPos,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            PrinterLanguage::EscPos => "escpos",
            PrinterLanguage::Zpl => "zpl",
            PrinterLanguage::Tspl => "tspl",
        }
    }

    pub fn is_label(&self) -> bool {
        !matches!(self, PrinterLanguage::EscPos)
    }
}

fn d_width() -> f32 {
    50.0
}
fn d_height() -> f32 {
    30.0
}
fn d_gap() -> f32 {
    2.0
}
fn d_dpi() -> u32 {
    203
}
fn d_copies() -> u32 {
    1
}
fn d_symbology() -> String {
    "code128".into()
}
fn d_true() -> bool {
    true
}

/// A label layout. Positions and sizes are in millimetres from the top-left.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LabelJob {
    #[serde(default = "d_width")]
    pub width_mm: f32,
    #[serde(default = "d_height")]
    pub height_mm: f32,
    #[serde(default = "d_gap")]
    pub gap_mm: f32,
    #[serde(default = "d_dpi")]
    pub dpi: u32,
    #[serde(default = "d_copies")]
    pub copies: u32,
    #[serde(default)]
    pub elements: Vec<LabelElement>,
    /// Pre-rendered ZPL/TSPL sent to the printer as-is (elements ignored).
    #[serde(default)]
    pub raw: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum LabelElement {
    #[serde(rename_all = "camelCase")]
    Text {
        x: f32,
        y: f32,
        text: String,
        /// Character height in mm (default 3).
        #[serde(default)]
        size_mm: Option<f32>,
    },
    #[serde(rename_all = "camelCase")]
    Barcode {
        x: f32,
        y: f32,
        data: String,
        /// code128 | ean13 | ean8 | upca | code39
        #[serde(default = "d_symbology")]
        symbology: String,
        #[serde(default)]
        height_mm: Option<f32>,
        #[serde(default = "d_true")]
        show_text: bool,
        /// Narrow bar width in dots (default 2).
        #[serde(default)]
        module_width: Option<u32>,
    },
    #[serde(rename_all = "camelCase")]
    Qrcode {
        x: f32,
        y: f32,
        data: String,
        /// Module size in dots (default 4).
        #[serde(default)]
        cell_size: Option<u32>,
    },
    #[serde(rename_all = "camelCase")]
    Box {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        #[serde(default)]
        thickness_mm: Option<f32>,
    },
    /// Filled rectangle — use a small height/width for horizontal/vertical rules.
    #[serde(rename_all = "camelCase")]
    Line { x: f32, y: f32, width: f32, height: f32 },
}

impl LabelJob {
    fn dots(&self, mm: f32) -> u32 {
        (mm * self.dpi as f32 / 25.4).round().max(0.0) as u32
    }

    /// Simple text label from receipt-style lines (one per row), used when a
    /// normal print job is routed to a label printer.
    pub fn from_lines(lines: &[String]) -> LabelJob {
        let elements: Vec<LabelElement> = lines
            .iter()
            .enumerate()
            .map(|(i, text)| LabelElement::Text {
                x: 2.0,
                y: 2.0 + i as f32 * 4.0,
                text: text.clone(),
                size_mm: Some(3.0),
            })
            .collect();
        LabelJob {
            width_mm: d_width(),
            height_mm: d_height().max(4.0 + lines.len() as f32 * 4.0),
            gap_mm: d_gap(),
            dpi: d_dpi(),
            copies: 1,
            elements,
            raw: None,
        }
    }

    pub fn test_label(target: &str) -> LabelJob {
        serde_json::from_value(serde_json::json!({
            "elements": [
                { "type": "text", "x": 2, "y": 2, "text": "** TEST LABEL **", "sizeMm": 4 },
                { "type": "text", "x": 2, "y": 8, "text": format!("PrintAgent v{}", env!("CARGO_PKG_VERSION")) },
                { "type": "text", "x": 2, "y": 12, "text": target },
                { "type": "line", "x": 2, "y": 16, "width": 46, "height": 0.3 },
                { "type": "barcode", "x": 2, "y": 18, "data": "123456789012", "heightMm": 7 }
            ]
        }))
        .expect("static test label")
    }

    pub fn render(&self, lang: PrinterLanguage) -> Vec<u8> {
        if let Some(raw) = self.raw.as_deref().filter(|r| !r.trim().is_empty()) {
            return raw.as_bytes().to_vec();
        }
        match lang {
            PrinterLanguage::Tspl => self.to_tspl().into_bytes(),
            _ => self.to_zpl().into_bytes(),
        }
    }

    pub fn to_zpl(&self) -> String {
        let mut z = String::new();
        z.push_str("^XA\n^CI28\n");
        z.push_str(&format!("^PW{}\n^LL{}\n^LH0,0\n", self.dots(self.width_mm), self.dots(self.height_mm)));
        for el in &self.elements {
            match el {
                LabelElement::Text { x, y, text, size_mm } => {
                    let h = self.dots(size_mm.unwrap_or(3.0)).max(10);
                    z.push_str(&format!(
                        "^FO{},{}^A0N,{},{}^FH^FD{}^FS\n",
                        self.dots(*x),
                        self.dots(*y),
                        h,
                        h,
                        zpl_escape(text)
                    ));
                }
                LabelElement::Barcode { x, y, data, symbology, height_mm, show_text, module_width } => {
                    let h = self.dots(height_mm.unwrap_or(10.0)).max(10);
                    let t = if *show_text { "Y" } else { "N" };
                    let cmd = match symbology.to_lowercase().as_str() {
                        "ean13" => format!("^BEN,{},{},N", h, t),
                        "ean8" => format!("^B8N,{},{},N", h, t),
                        "upca" => format!("^BUN,{},{},N,Y", h, t),
                        "code39" => format!("^B3N,N,{},{},N", h, t),
                        _ => format!("^BCN,{},{},N,N", h, t),
                    };
                    z.push_str(&format!(
                        "^FO{},{}^BY{}{}^FH^FD{}^FS\n",
                        self.dots(*x),
                        self.dots(*y),
                        module_width.unwrap_or(2).clamp(1, 10),
                        cmd,
                        zpl_escape(data)
                    ));
                }
                LabelElement::Qrcode { x, y, data, cell_size } => {
                    z.push_str(&format!(
                        "^FO{},{}^BQN,2,{}^FH^FDQA,{}^FS\n",
                        self.dots(*x),
                        self.dots(*y),
                        cell_size.unwrap_or(4).clamp(1, 10),
                        zpl_escape(data)
                    ));
                }
                LabelElement::Box { x, y, width, height, thickness_mm } => {
                    z.push_str(&format!(
                        "^FO{},{}^GB{},{},{}^FS\n",
                        self.dots(*x),
                        self.dots(*y),
                        self.dots(*width),
                        self.dots(*height),
                        self.dots(thickness_mm.unwrap_or(0.3)).max(1)
                    ));
                }
                LabelElement::Line { x, y, width, height } => {
                    let (w, h) = (self.dots(*width).max(1), self.dots(*height).max(1));
                    z.push_str(&format!("^FO{},{}^GB{},{},{}^FS\n", self.dots(*x), self.dots(*y), w, h, w.min(h)));
                }
            }
        }
        z.push_str(&format!("^PQ{}\n^XZ\n", self.copies.max(1)));
        z
    }

    pub fn to_tspl(&self) -> String {
        let mut t = String::new();
        let mut line = |s: String| {
            t.push_str(&s);
            t.push_str("\r\n");
        };
        line(format!("SIZE {} mm,{} mm", fmt_mm(self.width_mm), fmt_mm(self.height_mm)));
        line(format!("GAP {} mm,0 mm", fmt_mm(self.gap_mm)));
        line("DIRECTION 1".into());
        line("REFERENCE 0,0".into());
        line("CLS".into());
        for el in &self.elements {
            match el {
                LabelElement::Text { x, y, text, size_mm } => {
                    let (font, mult) = tspl_font(self.dots(size_mm.unwrap_or(3.0)).max(8));
                    line(format!(
                        "TEXT {},{},\"{}\",0,{},{},\"{}\"",
                        self.dots(*x),
                        self.dots(*y),
                        font,
                        mult,
                        mult,
                        tspl_escape(text)
                    ));
                }
                LabelElement::Barcode { x, y, data, symbology, height_mm, show_text, module_width } => {
                    let kind = match symbology.to_lowercase().as_str() {
                        "ean13" => "EAN13",
                        "ean8" => "EAN8",
                        "upca" => "UPCA",
                        "code39" => "39",
                        _ => "128",
                    };
                    let narrow = module_width.unwrap_or(2).clamp(1, 10);
                    let wide = if kind == "39" { narrow * 2 } else { narrow };
                    line(format!(
                        "BARCODE {},{},\"{}\",{},{},0,{},{},\"{}\"",
                        self.dots(*x),
                        self.dots(*y),
                        kind,
                        self.dots(height_mm.unwrap_or(10.0)).max(10),
                        if *show_text { 1 } else { 0 },
                        narrow,
                        wide,
                        tspl_escape(data)
                    ));
                }
                LabelElement::Qrcode { x, y, data, cell_size } => {
                    line(format!(
                        "QRCODE {},{},M,{},A,0,\"{}\"",
                        self.dots(*x),
                        self.dots(*y),
                        cell_size.unwrap_or(4).clamp(1, 10),
                        tspl_escape(data)
                    ));
                }
                LabelElement::Box { x, y, width, height, thickness_mm } => {
                    let (x0, y0) = (self.dots(*x), self.dots(*y));
                    line(format!(
                        "BOX {},{},{},{},{}",
                        x0,
                        y0,
                        x0 + self.dots(*width),
                        y0 + self.dots(*height),
                        self.dots(thickness_mm.unwrap_or(0.3)).max(1)
                    ));
                }
                LabelElement::Line { x, y, width, height } => {
                    line(format!(
                        "BAR {},{},{},{}",
                        self.dots(*x),
                        self.dots(*y),
                        self.dots(*width).max(1),
                        self.dots(*height).max(1)
                    ));
                }
            }
        }
        line(format!("PRINT {},1", self.copies.max(1)));
        t
    }
}

fn fmt_mm(v: f32) -> String {
    let s = format!("{:.1}", v);
    s.trim_end_matches(".0").to_string()
}

/// `^FH` hex-escapes so `^`, `~` and `_` in data can't break the format.
fn zpl_escape(s: &str) -> String {
    s.replace('_', "_5F").replace('^', "_5E").replace('~', "_7E")
}

/// TSPL escapes a double quote inside a string as `\["]`.
fn tspl_escape(s: &str) -> String {
    s.replace('"', "\\[\"]").replace(['\r', '\n'], " ")
}

/// Pick the largest built-in bitmap font no taller than `height` dots and a
/// multiplier to get close to the requested size.
fn tspl_font(height: u32) -> (&'static str, u32) {
    const FONTS: [(&str, u32); 5] = [("1", 12), ("2", 20), ("3", 24), ("4", 32), ("5", 48)];
    let (name, h) = FONTS
        .iter()
        .rev()
        .find(|(_, h)| *h <= height)
        .copied()
        .unwrap_or(FONTS[0]);
    let mult = ((height as f32 / h as f32).round() as u32).clamp(1, 10);
    (name, mult)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job() -> LabelJob {
        serde_json::from_value(serde_json::json!({
            "widthMm": 40, "heightMm": 25, "copies": 2,
            "elements": [
                { "type": "text", "x": 1, "y": 1, "text": "Chai \"Masala\" ^_~" },
                { "type": "barcode", "x": 1, "y": 6, "data": "2012345012347", "symbology": "ean13" },
                { "type": "qrcode", "x": 25, "y": 6, "data": "https://x.y/1" },
                { "type": "line", "x": 0, "y": 5, "width": 40, "height": 0.25 }
            ]
        }))
        .unwrap()
    }

    #[test]
    fn zpl_output() {
        let z = job().to_zpl();
        assert!(z.starts_with("^XA"));
        assert!(z.contains("^PW320"));
        assert!(z.contains("^FDChai \"Masala\" _5E_5F_7E^FS"));
        assert!(z.contains("^BEN,80,Y,N"));
        assert!(z.contains("^FDQA,https://x.y/1"));
        assert!(z.contains("^PQ2"));
        assert!(z.trim_end().ends_with("^XZ"));
    }

    #[test]
    fn tspl_output() {
        let t = job().to_tspl();
        assert!(t.starts_with("SIZE 40 mm,25 mm\r\n"));
        assert!(t.contains("TEXT 8,8,\"3\",0,1,1,\"Chai \\[\"]Masala\\[\"] ^_~\""));
        assert!(t.contains("BARCODE 8,48,\"EAN13\",80,1,0,2,2,\"2012345012347\""));
        assert!(t.contains("QRCODE 200,48,M,4,A,0,\"https://x.y/1\""));
        assert!(t.contains("BAR 0,40,320,2"));
        assert!(t.ends_with("PRINT 2,1\r\n"));
    }

    #[test]
    fn raw_passthrough_and_language_parse() {
        let mut j = job();
        j.raw = Some("^XA^FDhi^FS^XZ".into());
        assert_eq!(j.render(PrinterLanguage::Tspl), b"^XA^FDhi^FS^XZ".to_vec());
        assert_eq!(PrinterLanguage::parse(Some("TSPL")), PrinterLanguage::Tspl);
        assert_eq!(PrinterLanguage::parse(None), PrinterLanguage::EscPos);
    }
}
