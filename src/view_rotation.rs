//! Display-only quarter turns. Source pixels and file operations stay unchanged.
use crate::model::Crop;
use eframe::egui::{self, Color32, Pos2, Rect, Vec2};

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct Rotation(u8);
impl Rotation {
    pub fn turn(&mut self, clockwise: bool) { self.0=(self.0+if clockwise {1}else{3})%4; }
    pub fn swapped(self) -> bool { self.0%2==1 }
    pub fn is_default(self) -> bool { self.0==0 }
    pub fn inverse(self) -> Self { Self((4-self.0)%4) }
    pub fn dimensions(self, (w,h):(u32,u32)) -> (u32,u32) { if self.swapped() {(h,w)}else{(w,h)} }
    pub fn vector(self, v:Vec2) -> Vec2 {
        match self.0 {1=>Vec2::new(-v.y,v.x),2=>-v,3=>Vec2::new(v.y,-v.x),_=>v}
    }
    pub fn point(self, p:Pos2, center:Pos2) -> Pos2 { center+self.vector(p-center) }
    pub fn rect(self, rect:Rect, center:Pos2) -> Rect {
        let a=self.point(rect.min,center);let b=self.point(rect.max,center);
        Rect::from_min_max(a.min(b),a.max(b))
    }
    pub fn crop(self, c:Crop, (w,h):(u32,u32)) -> Crop {
        match self.0 {
            1=>Crop{x:h-c.y-c.h,y:c.x,w:c.h,h:c.w},
            2=>Crop{x:w-c.x-c.w,y:h-c.y-c.h,..c},
            3=>Crop{x:c.y,y:w-c.x-c.w,w:c.h,h:c.w},
            _=>c,
        }
    }
    pub fn mesh(self, texture:egui::TextureId, dest:Rect, uv:Rect, center:Pos2) -> egui::Mesh {
        let mut mesh=egui::Mesh::with_texture(texture);
        mesh.add_rect_with_uv(dest,uv,Color32::WHITE);
        for vertex in &mut mesh.vertices {vertex.pos=self.point(vertex.pos,center);}
        mesh
    }
    pub fn paint(self, painter:&egui::Painter, texture:egui::TextureId, dest:Rect, uv:Rect, center:Pos2) {
        painter.add(self.mesh(texture,dest,uv,center));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn turns_and_crops_round_trip_without_changing_source_coordinates() {
        let original=Crop{x:7,y:11,w:23,h:17};
        let mut r=Rotation::default();
        for _ in 0..4 {
            r.turn(true);
            let viewed=r.crop(original,(100,60));
            assert_eq!(r.inverse().crop(viewed,r.dimensions((100,60))),original);
        }
        assert!(r.is_default());
        r.turn(false); assert_eq!(r.dimensions((100,60)),(60,100));
        r.turn(true); assert!(r.is_default());
    }
    #[test]
    fn clockwise_mesh_preserves_uv_and_rotates_clipping_bounds() {
        let mut r=Rotation::default();r.turn(true);
        let center=Pos2::new(200.0,200.0);
        let source=Rect::from_center_size(center,Vec2::new(100.0,60.0));
        let uv=Rect::from_min_max(Pos2::ZERO,Pos2::new(1.0,1.0));
        let mesh=r.mesh(egui::TextureId::Managed(1),source,uv,center);
        assert_eq!(mesh.vertices[0].pos,r.point(source.left_top(),center));
        assert_eq!(mesh.vertices[0].uv,Pos2::ZERO);
        let rotated=r.rect(source,center);
        assert_eq!(rotated.size(),Vec2::new(60.0,100.0));
        assert_eq!(r.inverse().rect(rotated,center),source);
        assert_eq!(r.point(source.left_top(),center),rotated.right_top());
    }
}
