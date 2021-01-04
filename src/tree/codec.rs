use crate::tree::{node::*, secret_tree::*, *};

impl Codec for NodeType {
    fn encode(&self, buffer: &mut Vec<u8>) -> Result<(), CodecError> {
        (*self as u8).encode(buffer)?;
        Ok(())
    }
    fn decode(cursor: &mut Cursor) -> Result<Self, CodecError> {
        Ok(NodeType::from(u8::decode(cursor)?))
    }
}

impl Codec for Node {
    fn encode(&self, buffer: &mut Vec<u8>) -> Result<(), CodecError> {
        match self {
            Node::Leaf(kp) => {
                NodeType::Leaf.encode(buffer)?;
                kp.encode(buffer)?;
            }
            Node::Parent(parent) => {
                NodeType::Parent.encode(buffer)?;
                parent.encode(buffer)?;
            }
        };
        Ok(())
    }
    fn decode(cursor: &mut Cursor) -> Result<Self, CodecError> {
        let node_type = NodeType::decode(cursor)?;
        let node = match node_type {
            NodeType::Leaf => {
                let key_package = KeyPackage::decode(cursor)?;
                Node::Leaf(key_package)
            }
            NodeType::Parent => {
                let parent_node = ParentNode::decode(cursor)?;
                Node::Parent(parent_node)
            }
            NodeType::Default => return Err(CodecError::DecodingError),
        };
        Ok(node)
    }
}

impl Codec for UpdatePathNode {
    fn encode(&self, buffer: &mut Vec<u8>) -> Result<(), CodecError> {
        self.public_key.encode(buffer)?;
        encode_vec(VecSize::VecU32, buffer, &self.encrypted_path_secret)?;
        Ok(())
    }
    fn decode(cursor: &mut Cursor) -> Result<Self, CodecError> {
        let public_key = HPKEPublicKey::decode(cursor)?;
        let encrypted_path_secret = decode_vec(VecSize::VecU32, cursor)?;
        Ok(UpdatePathNode {
            public_key,
            encrypted_path_secret,
        })
    }
}

impl Codec for UpdatePath {
    fn encode(&self, buffer: &mut Vec<u8>) -> Result<(), CodecError> {
        self.leaf_key_package.encode(buffer)?;
        encode_vec(VecSize::VecU16, buffer, &self.nodes)?;
        Ok(())
    }
    fn decode(cursor: &mut Cursor) -> Result<Self, CodecError> {
        let leaf_key_package = KeyPackage::decode(cursor)?;
        let nodes = decode_vec(VecSize::VecU16, cursor)?;
        Ok(UpdatePath {
            leaf_key_package,
            nodes,
        })
    }
}

// ASTree Codecs

impl Codec for SecretTreeNode {
    fn encode(&self, buffer: &mut Vec<u8>) -> Result<(), CodecError> {
        self.secret.encode(buffer)?;
        Ok(())
    }
}
