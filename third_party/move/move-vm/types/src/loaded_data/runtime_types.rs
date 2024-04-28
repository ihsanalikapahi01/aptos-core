// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

#![allow(clippy::non_canonical_partial_ord_impl)]

use derivative::Derivative;
use itertools::Itertools;
use move_binary_format::{
    errors::{PartialVMError, PartialVMResult},
    file_format::{
        AbilitySet, SignatureToken, StructHandle, StructTypeParameter, TypeParameterIndex,
    },
};
use move_core_types::{identifier::Identifier, language_storage::ModuleId, vm_status::StatusCode};
use serde::Serialize;
use smallbitvec::SmallBitVec;
use smallvec::{smallvec, SmallVec};
use std::{
    cell::RefCell,
    cmp::max,
    collections::{btree_map, BTreeMap},
    fmt,
    fmt::Debug,
};
use triomphe::Arc as TriompheArc;

/// Maximum depth of a fully-instantiated type, excluding field types of structs.
#[cfg(not(test))]
const MAX_INSTANTIATED_TYPE_DEPTH: usize = 256;

/// Maximum number of nodes in a fully-instantiated type. This does not include
/// field types of structs.
#[cfg(not(test))]
const MAX_INSTANTIATED_TYPE_NODE_COUNT: usize = 128;

#[derive(PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Debug)]
/// A formula describing the value depth of a type, using (the depths of) the type parameters as inputs.
///
/// It has the form of `max(CBase, T1 + C1, T2 + C2, ..)` where `Ti` is the depth of the ith type parameter
/// and `Ci` is just some constant.
///
/// This form has a special property: when you compute the max of multiple formulae, you can normalize
/// them into a single formula.
pub struct DepthFormula {
    pub terms: Vec<(TypeParameterIndex, u64)>, // Ti + Ci
    pub constant: Option<u64>,                 // Cbase
}

impl DepthFormula {
    pub fn constant(constant: u64) -> Self {
        Self {
            terms: vec![],
            constant: Some(constant),
        }
    }

    pub fn type_parameter(tparam: TypeParameterIndex) -> Self {
        Self {
            terms: vec![(tparam, 0)],
            constant: None,
        }
    }

    pub fn normalize(formulas: Vec<Self>) -> Self {
        let mut var_map = BTreeMap::new();
        let mut constant_acc = None;
        for formula in formulas {
            let Self { terms, constant } = formula;
            for (var, cur_factor) in terms {
                var_map
                    .entry(var)
                    .and_modify(|prev_factor| *prev_factor = max(cur_factor, *prev_factor))
                    .or_insert(cur_factor);
            }
            match (constant_acc, constant) {
                (_, None) => (),
                (None, Some(_)) => constant_acc = constant,
                (Some(c1), Some(c2)) => constant_acc = Some(max(c1, c2)),
            }
        }
        Self {
            terms: var_map.into_iter().collect(),
            constant: constant_acc,
        }
    }

    pub fn subst(
        &self,
        mut map: BTreeMap<TypeParameterIndex, DepthFormula>,
    ) -> PartialVMResult<DepthFormula> {
        let Self { terms, constant } = self;
        let mut formulas = vec![];
        if let Some(constant) = constant {
            formulas.push(DepthFormula::constant(*constant))
        }
        for (t_i, c_i) in terms {
            let Some(mut u_form) = map.remove(t_i) else {
                return Err(
                    PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                        .with_message(format!("{t_i:?} missing mapping")),
                );
            };
            u_form.scale(*c_i);
            formulas.push(u_form)
        }
        Ok(DepthFormula::normalize(formulas))
    }

    pub fn solve(&self, tparam_depths: &[u64]) -> u64 {
        let Self { terms, constant } = self;
        let mut depth = constant.as_ref().copied().unwrap_or(0);
        for (t_i, c_i) in terms {
            depth = max(depth, tparam_depths[*t_i as usize].saturating_add(*c_i))
        }
        depth
    }

    pub fn scale(&mut self, c: u64) {
        let Self { terms, constant } = self;
        for (_t_i, c_i) in terms {
            *c_i = (*c_i).saturating_add(c);
        }
        if let Some(cbase) = constant.as_mut() {
            *cbase = (*cbase).saturating_add(c);
        }
    }
}

#[derive(Debug, Clone, Eq, Hash, PartialEq)]
pub struct StructType {
    pub idx: StructNameIndex,
    pub fields: Vec<Type>,
    pub field_names: Vec<Identifier>,
    pub phantom_ty_args_mask: SmallBitVec,
    pub abilities: AbilitySet,
    pub type_parameters: Vec<StructTypeParameter>,
    pub name: Identifier,
    pub module: ModuleId,
}

impl StructType {
    pub fn type_param_constraints(&self) -> impl ExactSizeIterator<Item = &AbilitySet> {
        self.type_parameters.iter().map(|param| &param.constraints)
    }

    // Check if the local struct handle is compatible with the defined struct type.
    pub fn check_compatibility(&self, struct_handle: &StructHandle) -> PartialVMResult<()> {
        if !struct_handle.abilities.is_subset(self.abilities) {
            return Err(
                PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                    .with_message("Ability definition of module mismatch".to_string()),
            );
        }

        if self.phantom_ty_args_mask.len() != struct_handle.type_parameters.len()
            || !self
                .phantom_ty_args_mask
                .iter()
                .zip(struct_handle.type_parameters.iter())
                .all(|(defined_is_phantom, local_type_parameter)| {
                    !local_type_parameter.is_phantom || defined_is_phantom
                })
        {
            return Err(
                PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR).with_message(
                    "Phantom type parameter definition of module mismatch".to_string(),
                ),
            );
        }

        Ok(())
    }
}

#[derive(Debug, Copy, Clone, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StructNameIndex(pub usize);

#[derive(Debug, Clone, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StructIdentifier {
    pub module: ModuleId,
    pub name: Identifier,
}

#[derive(Debug, Clone, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Type {
    Bool,
    U8,
    U64,
    U128,
    Address,
    Signer,
    Vector(TriompheArc<Type>),
    Struct {
        idx: StructNameIndex,
        ability: AbilityInfo,
    },
    StructInstantiation {
        idx: StructNameIndex,
        ty_args: TriompheArc<Vec<Type>>,
        ability: AbilityInfo,
    },
    Reference(Box<Type>),
    MutableReference(Box<Type>),
    TyParam(u16),
    U16,
    U32,
    U256,
}

pub struct TypePreorderTraversalIter<'a> {
    stack: SmallVec<[&'a Type; 32]>,
}

impl<'a> Iterator for TypePreorderTraversalIter<'a> {
    type Item = &'a Type;

    fn next(&mut self) -> Option<Self::Item> {
        use Type::*;

        match self.stack.pop() {
            Some(ty) => {
                match ty {
                    Signer
                    | Bool
                    | Address
                    | U8
                    | U16
                    | U32
                    | U64
                    | U128
                    | U256
                    | Struct { .. }
                    | TyParam(..) => (),

                    Reference(ty) | MutableReference(ty) => {
                        self.stack.push(ty);
                    },

                    Vector(ty) => {
                        self.stack.push(ty);
                    },

                    StructInstantiation { ty_args, .. } => self.stack.extend(ty_args.iter().rev()),
                }
                Some(ty)
            },
            None => None,
        }
    }
}

// Cache for the ability of struct. They will be ignored when comparing equality or Ord as they are just used for caching purpose.
#[derive(Derivative)]
#[derivative(Debug, Clone, Eq, Hash, PartialEq, Ord, PartialOrd)]
pub struct AbilityInfo {
    #[derivative(
        PartialEq = "ignore",
        Hash = "ignore",
        Ord = "ignore",
        PartialOrd = "ignore"
    )]
    base_ability_set: AbilitySet,

    #[derivative(
        PartialEq = "ignore",
        Hash = "ignore",
        Ord = "ignore",
        PartialOrd = "ignore"
    )]
    phantom_ty_args_mask: SmallBitVec,
}

impl AbilityInfo {
    pub fn struct_(ability: AbilitySet) -> Self {
        Self {
            base_ability_set: ability,
            phantom_ty_args_mask: SmallBitVec::new(),
        }
    }

    pub fn generic_struct(base_ability_set: AbilitySet, phantom_ty_args_mask: SmallBitVec) -> Self {
        Self {
            base_ability_set,
            phantom_ty_args_mask,
        }
    }
}

impl Type {
    fn clone_impl(&self, count: &mut usize, depth: usize) -> PartialVMResult<Type> {
        self.apply_subst(|idx, _, _| Ok(Type::TyParam(idx)), count, depth)
    }

    fn apply_subst<F>(&self, subst: F, count: &mut usize, depth: usize) -> PartialVMResult<Type>
    where
        F: Fn(u16, &mut usize, usize) -> PartialVMResult<Type> + Copy,
    {
        if *count >= MAX_INSTANTIATED_TYPE_NODE_COUNT {
            return Err(PartialVMError::new(StatusCode::TOO_MANY_TYPE_NODES));
        }
        if depth > MAX_INSTANTIATED_TYPE_DEPTH {
            return Err(PartialVMError::new(StatusCode::VM_MAX_TYPE_DEPTH_REACHED));
        }

        *count += 1;
        let res = match self {
            Type::TyParam(idx) => {
                // To avoid double-counting, revert counting the type parameter.
                *count -= 1;
                subst(*idx, count, depth)?
            },
            Type::Bool => Type::Bool,
            Type::U8 => Type::U8,
            Type::U16 => Type::U16,
            Type::U32 => Type::U32,
            Type::U64 => Type::U64,
            Type::U128 => Type::U128,
            Type::U256 => Type::U256,
            Type::Address => Type::Address,
            Type::Signer => Type::Signer,
            Type::Vector(ty) => {
                Type::Vector(TriompheArc::new(ty.apply_subst(subst, count, depth + 1)?))
            },
            Type::Reference(ty) => {
                Type::Reference(Box::new(ty.apply_subst(subst, count, depth + 1)?))
            },
            Type::MutableReference(ty) => {
                Type::MutableReference(Box::new(ty.apply_subst(subst, count, depth + 1)?))
            },
            Type::Struct { idx, ability } => Type::Struct {
                idx: *idx,
                ability: ability.clone(),
            },
            Type::StructInstantiation {
                idx,
                ty_args: instantiation,
                ability,
            } => {
                let mut inst = vec![];
                for ty in instantiation.iter() {
                    inst.push(ty.apply_subst(subst, count, depth + 1)?)
                }
                Type::StructInstantiation {
                    idx: *idx,
                    ty_args: TriompheArc::new(inst),
                    ability: ability.clone(),
                }
            },
        };
        Ok(res)
    }

    pub fn subst(&self, ty_args: &[Type]) -> PartialVMResult<Type> {
        Ok(self.subst_impl(ty_args)?.0)
    }

    fn subst_impl(&self, ty_args: &[Type]) -> PartialVMResult<(Type, usize)> {
        let mut count = 0;
        let ty = self.apply_subst(
            |idx, cnt, depth| match ty_args.get(idx as usize) {
                Some(ty) => ty.clone_impl(cnt, depth),
                None => Err(
                    PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                        .with_message(format!(
                            "type substitution failed: index out of bounds -- len {} got {}",
                            ty_args.len(),
                            idx
                        )),
                ),
            },
            &mut count,
            1,
        )?;
        Ok((ty, count))
    }

    pub fn check_vec_ref(&self, inner_ty: &Type, is_mut: bool) -> PartialVMResult<Type> {
        match self {
            Type::MutableReference(inner) => match &**inner {
                Type::Vector(inner) => {
                    inner.check_eq(inner_ty)?;
                    Ok(inner.as_ref().clone())
                },
                _ => Err(
                    PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                        .with_message("VecMutBorrow expects a vector reference".to_string())
                        .with_sub_status(move_core_types::vm_status::sub_status::unknown_invariant_violation::EPARANOID_FAILURE),
                ),
            },
            Type::Reference(inner) if !is_mut => match &**inner {
                Type::Vector(inner) => {
                    inner.check_eq(inner_ty)?;
                    Ok(inner.as_ref().clone())
                },
                _ => Err(
                    PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                        .with_message("VecMutBorrow expects a vector reference".to_string())
                        .with_sub_status(move_core_types::vm_status::sub_status::unknown_invariant_violation::EPARANOID_FAILURE),
                ),
            },
            _ => Err(
                PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                    .with_message("VecMutBorrow expects a vector reference".to_string())
                    .with_sub_status(move_core_types::vm_status::sub_status::unknown_invariant_violation::EPARANOID_FAILURE),
            ),
        }
    }

    pub fn check_eq(&self, other: &Self) -> PartialVMResult<()> {
        if self != other {
            return Err(
                PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                    .with_message(format!(
                        "Type mismatch: expected {:?}, got {:?}",
                        self, other
                    ))
                    .with_sub_status(move_core_types::vm_status::sub_status::unknown_invariant_violation::EPARANOID_FAILURE),
            );
        }
        Ok(())
    }

    pub fn check_ref_eq(&self, expected_inner: &Self) -> PartialVMResult<()> {
        match self {
            Type::MutableReference(inner) | Type::Reference(inner) => {
                inner.check_eq(expected_inner)
            },
            _ => Err(
                PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                    .with_message("VecMutBorrow expects a vector reference".to_string()),
            ),
        }
    }

    pub fn abilities(&self) -> PartialVMResult<AbilitySet> {
        match self {
            Type::Bool
            | Type::U8
            | Type::U16
            | Type::U32
            | Type::U64
            | Type::U128
            | Type::U256
            | Type::Address => Ok(AbilitySet::PRIMITIVES),

            // Technically unreachable but, no point in erroring if we don't have to
            Type::Reference(_) | Type::MutableReference(_) => Ok(AbilitySet::REFERENCES),
            Type::Signer => Ok(AbilitySet::SIGNER),

            Type::TyParam(_) => Err(PartialVMError::new(StatusCode::UNREACHABLE).with_message(
                "Unexpected TyParam type after translating from TypeTag to Type".to_string(),
            )),

            Type::Vector(ty) => {
                AbilitySet::polymorphic_abilities(AbilitySet::VECTOR, vec![false], vec![
                    ty.abilities()?
                ])
            },
            Type::Struct { ability, .. } => Ok(ability.base_ability_set),
            Type::StructInstantiation {
                ty_args,
                ability:
                    AbilityInfo {
                        base_ability_set,
                        phantom_ty_args_mask,
                    },
                ..
            } => {
                let type_argument_abilities = ty_args
                    .iter()
                    .map(|arg| arg.abilities())
                    .collect::<PartialVMResult<Vec<_>>>()?;
                AbilitySet::polymorphic_abilities(
                    *base_ability_set,
                    phantom_ty_args_mask.iter(),
                    type_argument_abilities,
                )
            },
        }
    }

    pub fn preorder_traversal(&self) -> TypePreorderTraversalIter<'_> {
        TypePreorderTraversalIter {
            stack: smallvec![self],
        }
    }

    /// Returns the number of nodes the type has.
    ///
    /// For example
    ///   - `u64` has one node
    ///   - `vector<u64>` has two nodes -- one for the vector and one for the element type u64.
    ///   - `Foo<u64, Bar<u8, bool>>` has 5 nodes.
    pub fn num_nodes(&self) -> usize {
        self.preorder_traversal().count()
    }

    /// Calculates the number of nodes in the substituted type.
    pub fn num_nodes_in_subst(&self, ty_args: &[Type]) -> PartialVMResult<usize> {
        use Type::*;

        thread_local! {
            static CACHE: RefCell<BTreeMap<usize, usize>> = RefCell::new(BTreeMap::new());
        }

        CACHE.with(|cache| {
            let mut cache = cache.borrow_mut();
            cache.clear();
            let mut num_nodes_in_arg = |idx: usize| -> PartialVMResult<usize> {
                Ok(match cache.entry(idx) {
                    btree_map::Entry::Occupied(entry) => *entry.into_mut(),
                    btree_map::Entry::Vacant(entry) => {
                        let ty = ty_args.get(idx).ok_or_else(|| {
                            PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                                .with_message(format!(
                                "type substitution failed: index out of bounds -- len {} got {}",
                                ty_args.len(),
                                idx
                            ))
                        })?;
                        *entry.insert(ty.num_nodes())
                    },
                })
            };

            let mut n = 0;
            for ty in self.preorder_traversal() {
                match ty {
                    TyParam(idx) => {
                        n += num_nodes_in_arg(*idx as usize)?;
                    },
                    Address
                    | Bool
                    | Signer
                    | U8
                    | U16
                    | U32
                    | U64
                    | U128
                    | U256
                    | Vector(..)
                    | Struct { .. }
                    | Reference(..)
                    | MutableReference(..)
                    | StructInstantiation { .. } => n += 1,
                }
            }

            Ok(n)
        })
    }
}

impl fmt::Display for StructIdentifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}::{}",
            self.module.short_str_lossless(),
            self.name.as_str()
        )
    }
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use Type::*;
        match self {
            Bool => f.write_str("bool"),
            U8 => f.write_str("u8"),
            U16 => f.write_str("u16"),
            U32 => f.write_str("u32"),
            U64 => f.write_str("u64"),
            U128 => f.write_str("u128"),
            U256 => f.write_str("u256"),
            Address => f.write_str("address"),
            Signer => f.write_str("signer"),
            Vector(et) => write!(f, "vector<{}>", et),
            Struct { idx, ability: _ } => write!(f, "s#{}", idx.0),
            StructInstantiation {
                idx,
                ty_args,
                ability: _,
            } => write!(
                f,
                "s#{}<{}>",
                idx.0,
                ty_args.iter().map(|t| t.to_string()).join(",")
            ),
            Reference(t) => write!(f, "&{}", t),
            MutableReference(t) => write!(f, "&mut {}", t),
            TyParam(no) => write!(f, "_{}", no),
        }
    }
}

#[derive(Clone, Serialize)]
pub struct TypeConfig {
    // Maximum number of nodes a fully-instantiated type has.
    max_ty_size: usize,
    // Maximum depth (in terms of number of nodes) a fully-instantiated type has.
    max_ty_depth: usize,
}

impl TypeConfig {
    pub fn production() -> Self {
        // TODO: pick the right parameters.
        Self::default()
    }
}

impl Default for TypeConfig {
    fn default() -> Self {
        Self {
            max_ty_size: 256,
            max_ty_depth: 256,
        }
    }
}

#[derive(Clone)]
pub struct TypeBuilder {
    #[allow(dead_code)]
    max_ty_size: usize,
    #[allow(dead_code)]
    max_ty_depth: usize,
}

impl TypeBuilder {
    pub fn new(ty_config: &TypeConfig) -> Self {
        Self {
            max_ty_size: ty_config.max_ty_size,
            max_ty_depth: ty_config.max_ty_depth,
        }
    }

    pub fn create_constant_ty(&self, const_tok: &SignatureToken) -> PartialVMResult<Type> {
        let mut count = 0;
        self.create_constant_ty_impl(const_tok, &mut count, 0)
    }

    fn create_constant_ty_impl(
        &self,
        const_tok: &SignatureToken,
        count: &mut usize,
        depth: usize,
    ) -> PartialVMResult<Type> {
        use SignatureToken as S;
        use Type as T;

        if *count >= self.max_ty_size {
            return Err(PartialVMError::new(StatusCode::TOO_MANY_TYPE_NODES));
        }
        if depth > self.max_ty_depth {
            return Err(PartialVMError::new(StatusCode::VM_MAX_TYPE_DEPTH_REACHED));
        }

        *count += 1;
        Ok(match const_tok {
            S::Bool => T::Bool,
            S::U8 => T::U8,
            S::U16 => T::U16,
            S::U32 => T::U32,
            S::U64 => T::U64,
            S::U128 => T::U128,
            S::U256 => T::U256,
            S::Address => T::Address,
            S::Vector(elem_tok) => {
                let elem_ty = self.create_constant_ty_impl(elem_tok, count, depth + 1)?;
                T::Vector(TriompheArc::new(elem_ty))
            },

            S::Struct(_) | S::StructInstantiation(_, _) => {
                return Err(
                    PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                        .with_message("Struct constants are not supported".to_string()),
                )
            },

            S::TypeParameter(_) | S::Reference(_) | S::MutableReference(_) | S::Signer => {
                return Err(
                    PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                        .with_message(
                            "Not allowed or not meaningful type for a constant".to_string(),
                        ),
                )
            },
        })
    }
}

// For tests, use smaller constants and ensure count is larger than depth.
#[cfg(test)]
const MAX_INSTANTIATED_TYPE_DEPTH: usize = 5;
#[cfg(test)]
const MAX_INSTANTIATED_TYPE_NODE_COUNT: usize = 11;

#[cfg(test)]
mod unit_tests {
    use super::*;
    use claims::{assert_err, assert_ok};

    fn struct_inst_for_test(ty_args: Vec<Type>) -> Type {
        Type::StructInstantiation {
            idx: StructNameIndex(0),
            ability: AbilityInfo::struct_(AbilitySet::EMPTY),
            ty_args: TriompheArc::new(ty_args),
        }
    }

    fn struct_for_test() -> Type {
        Type::Struct {
            idx: StructNameIndex(0),
            ability: AbilityInfo::struct_(AbilitySet::EMPTY),
        }
    }

    #[test]
    fn test_num_nodes_in_type() {
        use Type::*;

        let cases = [
            (U8, 1),
            (Vector(TriompheArc::new(U8)), 2),
            (Vector(TriompheArc::new(Vector(TriompheArc::new(U8)))), 3),
            (Reference(Box::new(Bool)), 2),
            (TyParam(0), 1),
            (struct_for_test(), 1),
            (struct_inst_for_test(vec![U8, U8]), 3),
            (
                struct_inst_for_test(vec![U8, struct_inst_for_test(vec![Bool, Bool, Bool]), U8]),
                7,
            ),
        ];

        for (ty, expected) in cases {
            assert_eq!(ty.num_nodes(), expected);
        }
    }

    #[test]
    fn test_num_nodes_in_subst() {
        use Type::*;

        let cases: Vec<(Type, Vec<Type>, usize)> = vec![
            (TyParam(0), vec![Bool], 1),
            (TyParam(0), vec![Vector(TriompheArc::new(Bool))], 2),
            (Bool, vec![], 1),
            (
                struct_inst_for_test(vec![TyParam(0), TyParam(0)]),
                vec![Vector(TriompheArc::new(Bool))],
                5,
            ),
            (
                struct_inst_for_test(vec![TyParam(0), TyParam(1)]),
                vec![
                    Vector(TriompheArc::new(Bool)),
                    Vector(TriompheArc::new(Vector(TriompheArc::new(Bool)))),
                ],
                6,
            ),
        ];

        for (ty, ty_args, expected) in cases {
            let num_nodes = ty.subst_impl(&ty_args).unwrap().1;
            assert_eq!(num_nodes, expected);
            assert_eq!(ty.num_nodes_in_subst(&ty_args).unwrap(), expected);
        }
    }

    #[test]
    fn test_substitution_large_depth() {
        use Type::*;

        let ty = Vector(TriompheArc::new(Vector(TriompheArc::new(TyParam(0)))));
        let ty_arg = Vector(TriompheArc::new(Vector(TriompheArc::new(Bool))));
        assert_ok!(ty.subst(&[ty_arg.clone()]));

        let ty_arg = Vector(TriompheArc::new(ty_arg));
        let err = assert_err!(ty.subst(&[ty_arg]));
        assert_eq!(err.major_status(), StatusCode::VM_MAX_TYPE_DEPTH_REACHED);
    }

    #[test]
    fn test_substitution_large_count() {
        use Type::*;

        let ty_params: Vec<Type> = (0..5).map(TyParam).collect();
        let ty = struct_inst_for_test(ty_params);

        // Each type argument contributes 2 nodes, so in total the count is 11.
        let ty_args: Vec<Type> = (0..5).map(|_| Vector(TriompheArc::new(Bool))).collect();
        let count = assert_ok!(ty.subst_impl(&ty_args)).1;
        assert_eq!(count, 11);

        let ty_args: Vec<Type> = (0..5)
            .map(|i| {
                if i == 4 {
                    // 3 nodes, to increase the total count to 12.
                    struct_inst_for_test(vec![U64, struct_for_test()])
                } else {
                    Vector(TriompheArc::new(Bool))
                }
            })
            .collect();
        let err = assert_err!(ty.subst(&ty_args));
        assert_eq!(err.major_status(), StatusCode::TOO_MANY_TYPE_NODES);
    }
}
