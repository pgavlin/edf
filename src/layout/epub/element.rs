use core::ops::Deref;
use ego_tree::NodeRef;
use scraper::node::Node;
use selectors::{
    attr::{AttrSelectorOperation, CaseSensitivity, NamespaceConstraint},
    matching, OpaqueElement,
};
use std::hash::{Hash, Hasher};
use style::servo::selector_parser;

#[derive(Eq, PartialEq, Debug, Copy, Clone)]
pub struct Element<'a>(NodeRef<'a, Node>);

impl<'a> Element<'a> {
    pub fn new(node: NodeRef<'a, Node>) -> Self {
        node.value().as_element().unwrap();
        Self(node)
    }

    pub fn value(&self) -> &'a scraper::node::Element {
        self.0.value().as_element().unwrap()
    }

    /// Returns the value of an attribute.
    pub fn attr(&self, attr: &str) -> Option<&'a str> {
        self.value().attr(attr)
    }
}

impl<'a> Deref for Element<'a> {
    type Target = NodeRef<'a, Node>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'a> From<NodeRef<'a, Node>> for Element<'a> {
    fn from(node: NodeRef<'a, Node>) -> Self {
        Element(node)
    }
}

impl<'a> Hash for Element<'a> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.id().hash(state)
    }
}

impl<'a> selectors::Element for Element<'a> {
    type Impl = selector_parser::SelectorImpl;

    /// Converts self into an opaque representation.
    fn opaque(&self) -> OpaqueElement {
        OpaqueElement::new(self.value())
    }

    fn parent_element(&self) -> Option<Self> {
        self.parent().and_then(|n| Some(n.into()))
    }

    /// Whether the parent node of this element is a shadow root.
    fn parent_node_is_shadow_root(&self) -> bool {
        false
    }

    /// The host of the containing shadow root, if any.
    fn containing_shadow_host(&self) -> Option<Self> {
        None
    }

    /// Whether we're matching on a pseudo-element.
    fn is_pseudo_element(&self) -> bool {
        false
    }

    /// Skips non-element nodes
    fn prev_sibling_element(&self) -> Option<Self> {
        self.prev_siblings()
            .find(|sibling| sibling.value().is_element())
            .map(|n| n.into())
    }

    /// Skips non-element nodes
    fn next_sibling_element(&self) -> Option<Self> {
        self.next_siblings()
            .find(|sibling| sibling.value().is_element())
            .map(|n| n.into())
    }

    /// Skips non-element nodes
    fn first_element_child(&self) -> Option<Self> {
        self.children()
            .find(|child| child.value().is_element())
            .map(|n| n.into())
    }

    fn is_html_element_in_html_document(&self) -> bool {
        // TODO(pdg) a real implementation would be nice.
        true
    }

    fn has_local_name(
        &self,
        local_name: &<Self::Impl as selectors::SelectorImpl>::BorrowedLocalName,
    ) -> bool {
        &self.value().name.local == local_name
    }

    /// Empty string for no namespace
    fn has_namespace(
        &self,
        ns: &<Self::Impl as selectors::SelectorImpl>::BorrowedNamespaceUrl,
    ) -> bool {
        &self.value().name.ns == ns
    }

    /// Whether this element and the `other` element have the same local name and namespace.
    fn is_same_type(&self, other: &Self) -> bool {
        self.value().name.local == other.value().name.local
            && self.value().name.ns == other.value().name.ns
    }

    fn attr_matches(
        &self,
        ns: &NamespaceConstraint<&<Self::Impl as selectors::SelectorImpl>::NamespaceUrl>,
        local_name: &<Self::Impl as selectors::SelectorImpl>::LocalName,
        operation: &AttrSelectorOperation<&<Self::Impl as selectors::SelectorImpl>::AttrValue>,
    ) -> bool {
        self.value().attrs.iter().any(|(key, value)| {
            !matches!(*ns, NamespaceConstraint::Specific(url) if **url != key.ns)
                && local_name.0 == key.local
                && operation.eval_str(value)
        })
    }

    fn match_non_ts_pseudo_class(
        &self,
        pc: &<Self::Impl as selectors::SelectorImpl>::NonTSPseudoClass,
        context: &mut matching::MatchingContext<Self::Impl>,
    ) -> bool {
        false
    }

    fn match_pseudo_element(
        &self,
        pe: &<Self::Impl as selectors::SelectorImpl>::PseudoElement,
        context: &mut matching::MatchingContext<Self::Impl>,
    ) -> bool {
        false
    }

    /// Sets selector flags on the elemnt itself or the parent, depending on the
    /// flags, which indicate what kind of work may need to be performed when
    /// DOM state changes.
    fn apply_selector_flags(&self, flags: matching::ElementSelectorFlags) {}

    /// Whether this element is a `link`.
    fn is_link(&self) -> bool {
        self.value().name() == "link"
    }

    /// Returns whether the element is an HTML <slot> element.
    fn is_html_slot_element(&self) -> bool {
        true
    }

    fn has_id(
        &self,
        id: &<Self::Impl as selectors::SelectorImpl>::Identifier,
        case_sensitivity: CaseSensitivity,
    ) -> bool {
        match self.value().id() {
            Some(val) => case_sensitivity.eq(id.0.as_bytes(), val.as_bytes()),
            None => false,
        }
    }

    fn has_class(
        &self,
        name: &<Self::Impl as selectors::SelectorImpl>::Identifier,
        case_sensitivity: CaseSensitivity,
    ) -> bool {
        self.value().has_class(
            &name.0,
            match case_sensitivity {
                CaseSensitivity::CaseSensitive => scraper::CaseSensitivity::CaseSensitive,
                CaseSensitivity::AsciiCaseInsensitive => {
                    scraper::CaseSensitivity::AsciiCaseInsensitive
                }
            },
        )
    }

    /// Returns the mapping from the `exportparts` attribute in the reverse
    /// direction, that is, in an outer-tree -> inner-tree direction.
    fn imported_part(
        &self,
        name: &<Self::Impl as selectors::SelectorImpl>::Identifier,
    ) -> Option<<Self::Impl as selectors::SelectorImpl>::Identifier> {
        None
    }

    fn is_part(&self, name: &<Self::Impl as selectors::SelectorImpl>::Identifier) -> bool {
        false
    }

    /// Returns whether this element matches `:empty`.
    ///
    /// That is, whether it does not contain any child element or any non-zero-length text node.
    /// See http://dev.w3.org/csswg/selectors-3/#empty-pseudo
    fn is_empty(&self) -> bool {
        !self
            .children()
            .any(|child| child.value().is_element() || child.value().is_text())
    }

    /// Returns whether this element matches `:root`,
    /// i.e. whether it is the root element of a document.
    ///
    /// Note: this can be false even if `.parent_element()` is `None`
    /// if the parent node is a `DocumentFragment`.
    fn is_root(&self) -> bool {
        self.parent()
            .map_or(false, |parent| parent.value().is_document())
    }
}
