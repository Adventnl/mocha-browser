//! Minimal DOM tree representation for Mocha Browser.
//!
//! This crate stores a tree of nodes in a flat arena ([`Document`]) and refers
//! to nodes by [`NodeId`]. It performs **no parsing** (that is `mocha_html`) and
//! **no layout** (that is `mocha_layout`). Invalid node ids return a
//! [`MochaError::Dom`] error rather than panicking.

use mocha_error::{MochaError, MochaResult};

/// A stable handle to a node inside a [`Document`].
///
/// The inner `usize` is an index into the document's arena. Ids are never reused
/// within a document, so a [`NodeId`] obtained from one document must not be used
/// with another.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId(pub usize);

/// The kind of a DOM node and its kind-specific data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeKind {
    /// The root document node.
    Document,
    /// An element such as `<p>` with a tag name and attributes.
    Element(ElementData),
    /// A run of text.
    Text(TextData),
    /// An HTML comment (`<!-- ... -->`), storing the comment body.
    Comment(String),
    /// A doctype declaration, storing the doctype string (for example `html`).
    Doctype(String),
}

/// Data carried by an [`NodeKind::Element`] node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ElementData {
    /// The lowercase tag name (for example `div`).
    pub tag_name: String,
    /// The element's attributes in source order.
    pub attributes: Vec<Attribute>,
}

impl ElementData {
    /// Return the value of the named attribute, if present.
    pub fn attribute(&self, name: &str) -> Option<&str> {
        self.attributes
            .iter()
            .find(|attribute| attribute.name == name)
            .map(|attribute| attribute.value.as_str())
    }
}

/// A single element attribute. Valueless attributes store an empty string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attribute {
    /// The attribute name.
    pub name: String,
    /// The attribute value, or an empty string for a valueless attribute.
    pub value: String,
}

/// Data carried by an [`NodeKind::Text`] node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextData {
    /// The text content.
    pub text: String,
}

/// A single node within a [`Document`] arena.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    /// What kind of node this is.
    pub kind: NodeKind,
    /// The parent node, or `None` for the document root.
    pub parent: Option<NodeId>,
    /// The child nodes in document order.
    pub children: Vec<NodeId>,
}

/// An arena-backed DOM tree.
///
/// Construct with [`Document::new`], which creates a single [`NodeKind::Document`]
/// root. Build the tree with `create_*` methods followed by [`Document::append_child`].
#[derive(Debug, Clone)]
pub struct Document {
    nodes: Vec<Node>,
    root_id: NodeId,
}

impl Document {
    /// Create a new document containing only a root [`NodeKind::Document`] node.
    pub fn new() -> Document {
        let root = Node {
            kind: NodeKind::Document,
            parent: None,
            children: Vec::new(),
        };
        Document {
            nodes: vec![root],
            root_id: NodeId(0),
        }
    }

    /// The id of the document root node.
    pub fn root_id(&self) -> NodeId {
        self.root_id
    }

    /// The total number of nodes in the arena (including the root).
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Always `false`: a document always contains at least the root node.
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Create a detached element node and return its id.
    ///
    /// The node has no parent until passed to [`Document::append_child`].
    pub fn create_element(
        &mut self,
        tag_name: impl Into<String>,
        attributes: Vec<Attribute>,
    ) -> NodeId {
        self.push(NodeKind::Element(ElementData {
            tag_name: tag_name.into(),
            attributes,
        }))
    }

    /// Create a detached text node and return its id.
    pub fn create_text(&mut self, text: impl Into<String>) -> NodeId {
        self.push(NodeKind::Text(TextData { text: text.into() }))
    }

    /// Create a detached comment node and return its id.
    pub fn create_comment(&mut self, text: impl Into<String>) -> NodeId {
        self.push(NodeKind::Comment(text.into()))
    }

    /// Create a detached doctype node and return its id.
    pub fn create_doctype(&mut self, text: impl Into<String>) -> NodeId {
        self.push(NodeKind::Doctype(text.into()))
    }

    /// Append `child` to `parent`, updating both relationships.
    ///
    /// Returns a [`MochaError::Dom`] error if either id is invalid, if the child
    /// is the document root, or if the child already has a parent.
    pub fn append_child(&mut self, parent: NodeId, child: NodeId) -> MochaResult<()> {
        self.check_id(parent)?;
        self.check_id(child)?;
        if child == self.root_id {
            return Err(MochaError::Dom(
                "cannot append the document root as a child".to_string(),
            ));
        }
        if let Some(existing) = self.nodes[child.0].parent {
            return Err(MochaError::Dom(format!(
                "node {} already has parent {}",
                child.0, existing.0
            )));
        }
        self.nodes[child.0].parent = Some(parent);
        self.nodes[parent.0].children.push(child);
        Ok(())
    }

    /// Detach `child` from `parent`, updating both relationships.
    ///
    /// Returns a [`MochaError::Dom`] error if either id is invalid or if `child`
    /// is not actually a child of `parent`.
    pub fn remove_child(&mut self, parent: NodeId, child: NodeId) -> MochaResult<()> {
        self.check_id(parent)?;
        self.check_id(child)?;
        let position = self.nodes[parent.0]
            .children
            .iter()
            .position(|&id| id == child)
            .ok_or_else(|| {
                MochaError::Dom(format!(
                    "node {} is not a child of node {}",
                    child.0, parent.0
                ))
            })?;
        self.nodes[parent.0].children.remove(position);
        self.nodes[child.0].parent = None;
        Ok(())
    }

    /// Borrow the node with the given id.
    pub fn node(&self, id: NodeId) -> MochaResult<&Node> {
        self.check_id(id)?;
        Ok(&self.nodes[id.0])
    }

    /// Return the children of the given node.
    pub fn children(&self, id: NodeId) -> MochaResult<&[NodeId]> {
        self.check_id(id)?;
        Ok(&self.nodes[id.0].children)
    }

    /// Return the parent of the given node, if any.
    pub fn parent(&self, id: NodeId) -> MochaResult<Option<NodeId>> {
        self.check_id(id)?;
        Ok(self.nodes[id.0].parent)
    }

    /// Collect node ids in depth-first pre-order starting at `start`.
    ///
    /// The start node is included first, followed by each subtree in child order.
    pub fn traverse_depth_first(&self, start: NodeId) -> MochaResult<Vec<NodeId>> {
        self.check_id(start)?;
        let mut order = Vec::new();
        // Explicit stack to avoid recursion; children pushed in reverse so the
        // first child is visited first.
        let mut stack = vec![start];
        while let Some(id) = stack.pop() {
            order.push(id);
            for &child in self.nodes[id.0].children.iter().rev() {
                stack.push(child);
            }
        }
        Ok(order)
    }

    fn push(&mut self, kind: NodeKind) -> NodeId {
        let id = NodeId(self.nodes.len());
        self.nodes.push(Node {
            kind,
            parent: None,
            children: Vec::new(),
        });
        id
    }

    fn check_id(&self, id: NodeId) -> MochaResult<()> {
        if id.0 < self.nodes.len() {
            Ok(())
        } else {
            Err(MochaError::Dom(format!("invalid node id: {}", id.0)))
        }
    }
}

impl Default for Document {
    fn default() -> Self {
        Document::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_document_has_root_node() {
        let document = Document::new();
        let root = document.node(document.root_id()).unwrap();
        assert_eq!(root.kind, NodeKind::Document);
        assert_eq!(root.parent, None);
        assert!(root.children.is_empty());
    }

    #[test]
    fn append_child_updates_parent_and_children() {
        let mut document = Document::new();
        let root = document.root_id();
        let element = document.create_element("p", Vec::new());
        document.append_child(root, element).unwrap();

        assert_eq!(document.children(root).unwrap(), &[element]);
        assert_eq!(document.parent(element).unwrap(), Some(root));
    }

    #[test]
    fn remove_child_updates_parent_and_children() {
        let mut document = Document::new();
        let root = document.root_id();
        let element = document.create_element("div", Vec::new());
        document.append_child(root, element).unwrap();
        document.remove_child(root, element).unwrap();

        assert!(document.children(root).unwrap().is_empty());
        assert_eq!(document.parent(element).unwrap(), None);
    }

    #[test]
    fn depth_first_traversal_order_is_correct() {
        // Tree: root -> [a -> [a1, a2], b]
        let mut document = Document::new();
        let root = document.root_id();
        let a = document.create_element("div", Vec::new());
        let b = document.create_element("div", Vec::new());
        let a1 = document.create_text("a1");
        let a2 = document.create_text("a2");
        document.append_child(root, a).unwrap();
        document.append_child(root, b).unwrap();
        document.append_child(a, a1).unwrap();
        document.append_child(a, a2).unwrap();

        let order = document.traverse_depth_first(root).unwrap();
        assert_eq!(order, vec![root, a, a1, a2, b]);
    }

    #[test]
    fn invalid_node_id_returns_error() {
        let document = Document::new();
        let error = document.node(NodeId(999)).unwrap_err();
        assert!(matches!(error, MochaError::Dom(_)));
    }

    #[test]
    fn element_attributes_are_stored() {
        let mut document = Document::new();
        let id = document.create_element(
            "div",
            vec![Attribute {
                name: "id".to_string(),
                value: "main".to_string(),
            }],
        );
        let node = document.node(id).unwrap();
        match &node.kind {
            NodeKind::Element(data) => {
                assert_eq!(data.tag_name, "div");
                assert_eq!(data.attribute("id"), Some("main"));
                assert_eq!(data.attribute("missing"), None);
            }
            other => panic!("expected element, got {other:?}"),
        }
    }

    #[test]
    fn text_node_stores_text() {
        let mut document = Document::new();
        let id = document.create_text("Hello Mocha");
        match &document.node(id).unwrap().kind {
            NodeKind::Text(data) => assert_eq!(data.text, "Hello Mocha"),
            other => panic!("expected text, got {other:?}"),
        }
    }

    #[test]
    fn append_rejects_already_parented_child() {
        let mut document = Document::new();
        let root = document.root_id();
        let parent_a = document.create_element("div", Vec::new());
        let parent_b = document.create_element("div", Vec::new());
        let child = document.create_text("x");
        document.append_child(root, parent_a).unwrap();
        document.append_child(root, parent_b).unwrap();
        document.append_child(parent_a, child).unwrap();

        let error = document.append_child(parent_b, child).unwrap_err();
        assert!(matches!(error, MochaError::Dom(_)));
    }

    #[test]
    fn remove_child_rejects_non_child() {
        let mut document = Document::new();
        let root = document.root_id();
        let stranger = document.create_text("x");
        let error = document.remove_child(root, stranger).unwrap_err();
        assert!(matches!(error, MochaError::Dom(_)));
    }
}
