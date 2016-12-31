/****************************************************************************
**
** svgcleaner could help you to clean up your SVG files
** from unnecessary data.
** Copyright (C) 2012-2016 Evgeniy Reizner
**
** This program is free software; you can redistribute it and/or modify
** it under the terms of the GNU General Public License as published by
** the Free Software Foundation; either version 2 of the License, or
** (at your option) any later version.
**
** This program is distributed in the hope that it will be useful,
** but WITHOUT ANY WARRANTY; without even the implied warranty of
** MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
** GNU General Public License for more details.
**
** You should have received a copy of the GNU General Public License along
** with this program; if not, write to the Free Software Foundation, Inc.,
** 51 Franklin Street, Fifth Floor, Boston, MA 02110-1301 USA.
**
****************************************************************************/

use task::short::{EId, AId, Unit};

use svgdom::{Document, Node, Attributes, AttributeValue};
use svgdom::types::{Length, Transform};

pub fn apply_transform_to_shapes(doc: &Document) {
    // If group has transform and contains only valid shapes
    // we can apply the group's transform to children before applying transform to
    // actual shape's coordinates.
    //
    // We use own implementation, because 'task::ungroup_groups' method does not support it.
    let iter = doc.descendants().svg().filter(|n|    n.is_tag_name(EId::G)
                                                  && n.has_attribute(AId::Transform));

    for node in iter {
        if !is_valid_transform(&node) || !is_valid_attrs(&node) {
            continue;
        }

        // check that all children is valid
        if node.children().svg().all(|n| {
            let flag = match n.tag_id().unwrap() {
                  EId::Rect
                | EId::Circle
                | EId::Ellipse
                | EId::Line => true,
                _ => false,
            };

            flag && is_valid_transform(&n) && is_valid_attrs(&n) && is_valid_coords(&n)
        }) {
            let ts = get_ts(&node);

            // apply group's transform to children
            for child in node.children().svg() {
                if child.has_attribute(AId::Transform) {
                    // we should multiply transform matrices
                    let mut ts1 = ts.clone();
                    let ts2 = get_ts(&child);
                    ts1.append(&ts2);
                    child.set_attribute(AId::Transform, ts1);
                } else {
                    child.set_attribute(AId::Transform, ts);
                }
            }

            node.remove_attribute(AId::Transform);

            // we do not remove group element here, because
            // it's 'task::ungroup_groups' job
        }
    }

    // apply transform to shapes
    let iter = doc.descendants().svg().filter(|n| n.has_attribute(AId::Transform));
    for node in iter {
        match node.tag_id().unwrap() {
            EId::Rect => process_rect(&node),
            EId::Circle => process_circle(&node),
            EId::Ellipse => process_ellipse(&node),
            EId::Line => process_line(&node),
            _ => {}
        }
    }
}

fn process<F>(node: &Node, func: F)
    where F : Fn(&mut Attributes, &Transform)
{
    if !is_valid_transform(node) || !is_valid_attrs(node) || !is_valid_coords(node) {
        return;
    }

    let ts = get_ts(node);

    {
        let mut attrs = node.attributes_mut();
        func(&mut attrs, &ts);
        attrs.remove(AId::Transform);
    }

    if ts.has_scale() {
        // we must update 'stroke-width' if transform had scale part in it
        let (sx, _) = ts.get_scale();
        ::task::utils::recalc_stroke_width(node, sx);
    }
}

fn process_rect(node: &Node) {
    process(node, |mut attrs, ts| {
        scale_pos_coord(&mut attrs, AId::X, AId::Y, &ts);

        if ts.has_scale() {
            let (sx, _) = ts.get_scale();

            scale_coord(&mut attrs, AId::Width, &sx);
            scale_coord(&mut attrs, AId::Height, &sx);

            scale_coord(&mut attrs, AId::Rx, &sx);
            scale_coord(&mut attrs, AId::Ry, &sx);
        }
    });
}

fn process_circle(node: &Node) {
    process(node, |mut attrs, ts| {
        scale_pos_coord(&mut attrs, AId::Cx, AId::Cy, &ts);

        if ts.has_scale() {
            let (sx, _) = ts.get_scale();
            scale_coord(&mut attrs, AId::R, &sx);
        }
    });
}

fn process_ellipse(node: &Node) {
    process(node, |mut attrs, ts| {
        scale_pos_coord(&mut attrs, AId::Cx, AId::Cy, &ts);

        if ts.has_scale() {
            let (sx, _) = ts.get_scale();
            scale_coord(&mut attrs, AId::Rx, &sx);
            scale_coord(&mut attrs, AId::Ry, &sx);
        }
    });
}

fn process_line(node: &Node) {
    process(node, |mut attrs, ts| {
        scale_pos_coord(&mut attrs, AId::X1, AId::Y1, &ts);
        scale_pos_coord(&mut attrs, AId::X2, AId::Y2, &ts);
    });
}

fn is_valid_transform(node: &Node) -> bool {
    if !node.has_attribute(AId::Transform) {
        return true;
    }

    let ts = get_ts(node);

    // If transform has non-proportional scale - we should skip it,
    // because it can be applied only to a raster.
    if ts.has_scale() && !ts.has_proportional_scale() {
        return false;
    }

    // If transform has skew part - we should skip it,
    // because it can be applied only to a raster.
    if ts.has_skew() {
        return false;
    }

    return true;
}

// Element shouldn't have any linked elements, because they also must be transformed.
// TODO: process 'fill', 'stroke' and 'filter' linked elements only if they
//       used only by this element.
fn is_valid_attrs(node: &Node) -> bool {
    let attrs = node.attributes();

    if let Some(&AttributeValue::FuncLink(_)) = attrs.get_value(AId::Fill) {
        return false;
    }

    if let Some(&AttributeValue::FuncLink(_)) = attrs.get_value(AId::Stroke) {
        return false;
    }

    if let Some(&AttributeValue::FuncLink(_)) = attrs.get_value(AId::Filter) {
        return false;
    }

    if attrs.contains(AId::Mask) || attrs.contains(AId::ClipPath) {
        return false;
    }

    return true;
}

// We can process only coordinates without units.
fn is_valid_coords(node: &Node) -> bool {
    match node.tag_id().unwrap() {
        EId::Rect =>    _is_valid_coords(node, &[AId::X, AId::Y]),
        EId::Circle =>  _is_valid_coords(node, &[AId::Cx, AId::Cy]),
        EId::Ellipse => _is_valid_coords(node, &[AId::Cx, AId::Cy]),
        EId::Line =>    _is_valid_coords(node, &[AId::X1, AId::Y1, AId::X2, AId::Y2]),
        _ => false,
    }
}

fn _is_valid_coords(node: &Node, attr_ids: &[AId]) -> bool {
    let attrs = node.attributes();

    fn is_valid_coord(attrs: &Attributes, aid: AId) -> bool {
        if let Some(&AttributeValue::Length(v)) = attrs.get_value(aid) {
            v.unit == Unit::None
        } else {
            true
        }
    }

    for id in attr_ids {
        if !is_valid_coord(&attrs, *id) {
            return false;
        }
    }

    true
}

fn scale_pos_coord(attrs: &mut Attributes, aid_x: AId, aid_y: AId, ts: &Transform) {
    let x = get_value!(attrs, Length, aid_x, Length::zero());
    let y = get_value!(attrs, Length, aid_y, Length::zero());

    debug_assert!(x.unit == Unit::None);
    debug_assert!(y.unit == Unit::None);

    let (nx, ny) = ts.apply(x.num, y.num);
    attrs.insert_from(aid_x, (nx, Unit::None));
    attrs.insert_from(aid_y, (ny, Unit::None));
}

fn scale_coord(attrs: &mut Attributes, aid: AId, scale_factor: &f64) {
    if let Some(&mut AttributeValue::Length(ref mut len)) = attrs.get_value_mut(aid) {
        len.num *= *scale_factor;
    }
}

fn get_ts(node: &Node) -> Transform {
    *node.attribute_value(AId::Transform).unwrap().as_transform().unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    use svgdom::{Document, WriteToString};
    use task::resolve_attributes;

    macro_rules! test {
        ($name:ident, $in_text:expr, $out_text:expr) => (
            #[test]
            fn $name() {
                let doc = Document::from_data($in_text).unwrap();
                resolve_attributes(&doc).unwrap();
                apply_transform_to_shapes(&doc);
                assert_eq_text!(doc.to_string_with_opt(&write_opt_for_tests!()), $out_text);
            }
        )
    }

    macro_rules! test_eq {
        ($name:ident, $in_text:expr) => (
            test!($name, $in_text, String::from_utf8_lossy($in_text));
        )
    }

    test!(apply_1,
b"<svg>
    <rect height='10' width='10' x='10' y='10' transform='translate(10 20)'/>
</svg>",
"<svg>
    <rect height='10' width='10' x='20' y='30'/>
</svg>
");

    test!(apply_2,
b"<svg>
    <rect height='10' rx='2' ry='2' width='10' x='10' y='10' transform='translate(10 20) scale(2)'/>
</svg>",
"<svg>
    <rect height='20' rx='4' ry='4' stroke-width='2' width='20' x='30' y='40'/>
</svg>
");

    test!(apply_3,
b"<svg>
    <rect height='10' width='10' transform='translate(10 20) scale(2)'/>
</svg>",
"<svg>
    <rect height='20' stroke-width='2' width='20' x='10' y='20'/>
</svg>
");

    test!(apply_4,
b"<svg stroke-width='2'>
    <rect height='10' width='10' transform='scale(2)'/>
</svg>",
"<svg stroke-width='2'>
    <rect height='20' stroke-width='4' width='20' x='0' y='0'/>
</svg>
");

    test!(apply_circle_1,
b"<svg>
    <circle cx='10' cy='10' r='15' transform='translate(10 20) scale(2)'/>
</svg>",
"<svg>
    <circle cx='30' cy='40' r='30' stroke-width='2'/>
</svg>
");

    test!(apply_ellipse_1,
b"<svg>
    <ellipse cx='10' cy='10' rx='15' ry='15' transform='translate(10 20) scale(2)'/>
</svg>",
"<svg>
    <ellipse cx='30' cy='40' rx='30' ry='30' stroke-width='2'/>
</svg>
");

    test!(apply_line_1,
b"<svg>
    <line x1='10' x2='10' y1='15' y2='15' transform='translate(10 20) scale(2)'/>
</svg>",
"<svg>
    <line stroke-width='2' x1='30' x2='30' y1='50' y2='50'/>
</svg>
");

    test!(apply_g_1,
b"<svg>
    <g transform='translate(10 20) scale(2)'>
        <rect height='10' width='10' x='10' y='10' transform='scale(2)'/>
        <rect height='10' width='10' x='10' y='10'/>
        <rect height='10' width='10' x='10' y='10'/>
    </g>
</svg>",
"<svg>
    <g>
        <rect height='40' stroke-width='4' width='40' x='50' y='60'/>
        <rect height='20' stroke-width='2' width='20' x='30' y='40'/>
        <rect height='20' stroke-width='2' width='20' x='30' y='40'/>
    </g>
</svg>
");

    // ignore shapes with invalid coordinates units
    test_eq!(keep_1,
b"<svg>
    <rect height='10' transform='scale(2)' width='10' x='10in' y='10'/>
</svg>
"
);

    // ignore groups processing with invalid transform types
    // and attributes
    test_eq!(keep_2,
b"<svg>
    <g transform='scale(2 3)'>
        <rect height='10' width='10' x='10' y='10'/>
    </g>
    <mask id='m'/>
    <g mask='url(#m)' transform='scale(2)'>
        <rect height='10' width='10' x='10' y='10'/>
    </g>
</svg>
"
);

}