use itertools::Itertools;
use num_traits::{One, Zero};
use strum::EnumCount;
use strum_macros::{Display, EnumCount as EnumCountMacro, EnumIter};
use twenty_first::shared_math::b_field_element::BFieldElement;
use twenty_first::shared_math::mpolynomial::{Degree, MPolynomial};
use twenty_first::shared_math::x_field_element::XFieldElement;

use crate::cross_table_arguments::{CrossTableArg, EvalArg, PermArg};
use crate::fri_domain::FriDomain;
use crate::table::base_table::Extendable;

use super::base_table::{InheritsFromTable, Table, TableLike};
use super::challenges::{AllChallenges, TableChallenges};
use super::constraint_circuit::DualRowIndicator::{self, *};
use super::constraint_circuit::{
    ConstraintCircuit, ConstraintCircuitBuilder, ConstraintCircuitMonad,
};
use super::extension_table::{ExtensionTable, Quotientable, QuotientableExtensionTable};
use super::table_column::InstructionBaseTableColumn::{self, *};
use super::table_column::InstructionExtTableColumn::{self, *};

pub const INSTRUCTION_TABLE_NUM_PERMUTATION_ARGUMENTS: usize = 1;
pub const INSTRUCTION_TABLE_NUM_EVALUATION_ARGUMENTS: usize = 1;

pub const BASE_WIDTH: usize = InstructionBaseTableColumn::COUNT;
pub const FULL_WIDTH: usize = BASE_WIDTH + InstructionExtTableColumn::COUNT;

#[derive(Debug, Clone)]
pub struct InstructionTable {
    inherited_table: Table<BFieldElement>,
}

#[derive(Debug, Copy, Clone, Display, EnumCountMacro, EnumIter, PartialEq, Hash, Eq)]
pub enum InstructionTableChallengeId {
    ProcessorPermIndeterminate,
    IpProcessorWeight,
    CiProcessorWeight,
    NiaProcessorWeight,
    ProgramEvalIndeterminate,
    AddressWeight,
    InstructionWeight,
    NextInstructionWeight,
}

impl From<InstructionTableChallengeId> for usize {
    fn from(val: InstructionTableChallengeId) -> Self {
        val as usize
    }
}

#[derive(Debug, Clone)]
pub struct InstructionTableChallenges {
    /// The weight that combines two consecutive rows in the
    /// permutation/evaluation column of the instruction table.
    pub processor_perm_indeterminate: XFieldElement,

    /// Weights for condensing part of a row into a single column. (Related to processor table.)
    pub ip_processor_weight: XFieldElement,
    pub ci_processor_weight: XFieldElement,
    pub nia_processor_weight: XFieldElement,

    /// The weight that combines two consecutive rows in the
    /// permutation/evaluation column of the instruction table.
    pub program_eval_indeterminate: XFieldElement,

    /// Weights for condensing part of a row into a single column. (Related to program table.)
    pub address_weight: XFieldElement,
    pub instruction_weight: XFieldElement,
    pub next_instruction_weight: XFieldElement,
}

impl TableChallenges for InstructionTableChallenges {
    type Id = InstructionTableChallengeId;

    #[inline]
    fn get_challenge(&self, id: Self::Id) -> XFieldElement {
        match id {
            InstructionTableChallengeId::ProcessorPermIndeterminate => {
                self.processor_perm_indeterminate
            }
            InstructionTableChallengeId::IpProcessorWeight => self.ip_processor_weight,
            InstructionTableChallengeId::CiProcessorWeight => self.ci_processor_weight,
            InstructionTableChallengeId::NiaProcessorWeight => self.nia_processor_weight,
            InstructionTableChallengeId::ProgramEvalIndeterminate => {
                self.program_eval_indeterminate
            }
            InstructionTableChallengeId::AddressWeight => self.address_weight,
            InstructionTableChallengeId::InstructionWeight => self.instruction_weight,
            InstructionTableChallengeId::NextInstructionWeight => self.next_instruction_weight,
        }
    }
}

impl InheritsFromTable<BFieldElement> for InstructionTable {
    fn inherited_table(&self) -> &Table<BFieldElement> {
        &self.inherited_table
    }

    fn mut_inherited_table(&mut self) -> &mut Table<BFieldElement> {
        &mut self.inherited_table
    }
}

#[derive(Debug, Clone)]
pub struct ExtInstructionTable {
    pub(crate) inherited_table: Table<XFieldElement>,
}

impl Default for ExtInstructionTable {
    fn default() -> Self {
        Self {
            inherited_table: Table::new(
                BASE_WIDTH,
                FULL_WIDTH,
                vec![],
                "EmptyExtInstructionTable".to_string(),
            ),
        }
    }
}

impl Quotientable for ExtInstructionTable {}
impl QuotientableExtensionTable for ExtInstructionTable {}

impl InheritsFromTable<XFieldElement> for ExtInstructionTable {
    fn inherited_table(&self) -> &Table<XFieldElement> {
        &self.inherited_table
    }

    fn mut_inherited_table(&mut self) -> &mut Table<XFieldElement> {
        &mut self.inherited_table
    }
}

impl TableLike<BFieldElement> for InstructionTable {}

impl Extendable for InstructionTable {
    fn get_padding_rows(&self) -> (Option<usize>, Vec<Vec<BFieldElement>>) {
        let zero = BFieldElement::zero();
        let one = BFieldElement::one();
        if let Some(row) = self.data().last() {
            let mut padding_row = row.clone();
            // address keeps increasing
            padding_row[usize::from(Address)] += one;
            padding_row[usize::from(IsPadding)] = one;
            (None, vec![padding_row])
        } else {
            let mut padding_row = [zero; BASE_WIDTH];
            padding_row[usize::from(IsPadding)] = one;
            (None, vec![padding_row.to_vec()])
        }
    }
}

impl TableLike<XFieldElement> for ExtInstructionTable {}

impl ExtInstructionTable {
    fn ext_initial_constraints(
        challenges: &InstructionTableChallenges,
    ) -> Vec<MPolynomial<XFieldElement>> {
        let variables: Vec<MPolynomial<XFieldElement>> = MPolynomial::variables(FULL_WIDTH);

        let running_evaluation_initial =
            MPolynomial::from_constant(EvalArg::default_initial(), FULL_WIDTH);
        let running_product_initial =
            MPolynomial::from_constant(PermArg::default_initial(), FULL_WIDTH);

        let ip = variables[usize::from(Address)].clone();
        let ci = variables[usize::from(CI)].clone();
        let nia = variables[usize::from(NIA)].clone();
        let running_evaluation = variables[usize::from(RunningEvaluation)].clone();
        let running_product = variables[usize::from(RunningProductPermArg)].clone();

        let compressed_row_for_eval_arg = ip.scalar_mul(challenges.address_weight)
            + ci.scalar_mul(challenges.instruction_weight)
            + nia.scalar_mul(challenges.next_instruction_weight);

        let first_address_is_zero = ip;

        let running_evaluation_is_initialized_correctly = running_evaluation
            - running_evaluation_initial.scalar_mul(challenges.program_eval_indeterminate)
            - compressed_row_for_eval_arg;

        // due to the way the instruction table is constructed, the running product does not update
        // in the first row
        let running_product_is_initialized_correctly = running_product - running_product_initial;

        vec![
            first_address_is_zero,
            running_evaluation_is_initialized_correctly,
            running_product_is_initialized_correctly,
        ]
    }

    fn ext_consistency_constraints(
        _challenges: &InstructionTableChallenges,
    ) -> Vec<MPolynomial<XFieldElement>> {
        let one = MPolynomial::from_constant(XFieldElement::one(), FULL_WIDTH);

        let variables = MPolynomial::variables(FULL_WIDTH);
        let is_padding = variables[usize::from(IsPadding)].clone();

        let is_padding_is_bit = is_padding.clone() * (is_padding - one);

        vec![is_padding_is_bit]
    }

    pub fn ext_transition_constraints_as_circuits(
    ) -> Vec<ConstraintCircuit<InstructionTableChallenges, DualRowIndicator<FULL_WIDTH>>> {
        let circuit_builder: ConstraintCircuitBuilder<
            InstructionTableChallenges,
            DualRowIndicator<FULL_WIDTH>,
        > = ConstraintCircuitBuilder::new(2 * FULL_WIDTH);
        let one: ConstraintCircuitMonad<InstructionTableChallenges, DualRowIndicator<FULL_WIDTH>> =
            circuit_builder.b_constant(1u32.into());
        let addr = circuit_builder.input(CurrentRow(Address.into()));

        let addr_next = circuit_builder.input(NextRow(Address.into()));
        let current_instruction = circuit_builder.input(CurrentRow(CI.into()));
        let current_instruction_next = circuit_builder.input(NextRow(CI.into()));
        let next_instruction = circuit_builder.input(CurrentRow(NIA.into()));
        let next_instruction_next = circuit_builder.input(NextRow(NIA.into()));
        let is_padding = circuit_builder.input(CurrentRow(IsPadding.into()));
        let is_padding_next = circuit_builder.input(NextRow(IsPadding.into()));

        // Base table constraints
        let address_increases_by_one = addr_next.clone() - (addr.clone() + one.clone());
        let address_increases_by_one_or_ci_stays = address_increases_by_one.clone()
            * (current_instruction_next.clone() - current_instruction);
        let address_increases_by_one_or_nia_stays =
            address_increases_by_one.clone() * (next_instruction_next.clone() - next_instruction);
        let is_padding_is_0_or_remains_unchanged =
            is_padding.clone() * (is_padding_next.clone() - is_padding);

        // Extension table constraints
        let processor_perm_indeterminate =
            circuit_builder.challenge(InstructionTableChallengeId::ProcessorPermIndeterminate);
        let running_evaluation = circuit_builder.input(CurrentRow(RunningEvaluation.into()));
        let running_evaluation_next = circuit_builder.input(NextRow(RunningEvaluation.into()));

        let running_product = circuit_builder.input(CurrentRow(RunningProductPermArg.into()));
        let running_product_next = circuit_builder.input(NextRow(RunningProductPermArg.into()));

        // The running evaluation is updated if and only if
        // 1. the address changes, and
        // 2. the current row is not a padding row.
        // Stated differently:
        // 1. the address doesn't change
        //      or the current row is a padding row
        //      or the running evaluation is updated,
        // 2. the address does change
        //      or the running evaluation is not updated, and
        // 3. the current row is not a padding row
        //      or the running evaluation is not updated.
        let compressed_row_for_eval_arg = circuit_builder
            .challenge(InstructionTableChallengeId::AddressWeight)
            * addr_next.clone()
            + circuit_builder.challenge(InstructionTableChallengeId::InstructionWeight)
                * current_instruction_next.clone()
            + circuit_builder.challenge(InstructionTableChallengeId::NextInstructionWeight)
                * next_instruction_next.clone();

        let address_stays = addr_next.clone() - addr;
        let running_evaluations_stays =
            running_evaluation_next.clone() - running_evaluation.clone();
        let running_evaluation_update = running_evaluation_next
            - circuit_builder.challenge(InstructionTableChallengeId::ProgramEvalIndeterminate)
                * running_evaluation
            - compressed_row_for_eval_arg;

        let running_evaluation_is_well_formed = address_stays.clone()
            * (one.clone() - is_padding_next.clone())
            * running_evaluation_update
            + address_increases_by_one.clone() * running_evaluations_stays.clone()
            + is_padding_next.clone() * running_evaluations_stays;

        // The running product is updated if and only if
        // 1. the address doesn't change, and
        // 2. the current row is not a padding row.
        // Stated differently:
        // 1. the address does change
        //      or the current row is a padding row
        //      or the running product is updated,
        // 2. the address doesn't change
        //      or the running product is not updated, and
        // 3. the current row is not a padding row
        //      or the running product is not updated.
        let compressed_row_for_perm_arg =
            circuit_builder.challenge(InstructionTableChallengeId::IpProcessorWeight) * addr_next
                + circuit_builder.challenge(InstructionTableChallengeId::CiProcessorWeight)
                    * current_instruction_next
                + circuit_builder.challenge(InstructionTableChallengeId::NiaProcessorWeight)
                    * next_instruction_next;

        let running_product_stays = running_product_next.clone() - running_product.clone();
        let running_product_update = running_product_next
            - running_product * (processor_perm_indeterminate - compressed_row_for_perm_arg);

        let running_product_is_well_formed =
            address_increases_by_one * (one - is_padding_next.clone()) * running_product_update
                + address_stays * running_product_stays.clone()
                + is_padding_next * running_product_stays;

        vec![
            address_increases_by_one_or_ci_stays.consume(),
            address_increases_by_one_or_nia_stays.consume(),
            is_padding_is_0_or_remains_unchanged.consume(),
            running_evaluation_is_well_formed.consume(),
            running_product_is_well_formed.consume(),
        ]
    }

    fn ext_transition_constraints(
        challenges: &InstructionTableChallenges,
    ) -> Vec<MPolynomial<XFieldElement>> {
        let circuits = Self::ext_transition_constraints_as_circuits();
        circuits
            .into_iter()
            .map(|circ| circ.partial_evaluate(challenges))
            .collect_vec()
    }

    fn ext_terminal_constraints(
        _challenges: &InstructionTableChallenges,
    ) -> Vec<MPolynomial<XFieldElement>> {
        // no further constraints
        vec![]
    }
}

impl InstructionTable {
    pub fn new_prover(matrix: Vec<Vec<BFieldElement>>) -> Self {
        let inherited_table = Table::new(
            BASE_WIDTH,
            FULL_WIDTH,
            matrix,
            "InstructionTable".to_string(),
        );
        Self { inherited_table }
    }

    pub fn to_fri_domain_table(
        &self,
        fri_domain: &FriDomain<BFieldElement>,
        omicron: BFieldElement,
        padded_height: usize,
        num_trace_randomizers: usize,
    ) -> Self {
        let base_columns = 0..self.base_width();
        let fri_domain_codewords = self.low_degree_extension(
            fri_domain,
            omicron,
            padded_height,
            num_trace_randomizers,
            base_columns,
        );
        let inherited_table = self.inherited_table.with_data(fri_domain_codewords);
        Self { inherited_table }
    }

    pub fn extend(
        &self,
        challenges: &InstructionTableChallenges,
        interpolant_degree: Degree,
    ) -> ExtInstructionTable {
        let mut extension_matrix: Vec<Vec<XFieldElement>> = Vec::with_capacity(self.data().len());
        let mut processor_table_running_product = PermArg::default_initial();
        let mut program_table_running_evaluation = EvalArg::default_initial();
        let mut previous_row: Option<Vec<_>> = None;

        for row in self.data().iter() {
            let mut extension_row = [0.into(); FULL_WIDTH];
            extension_row[..BASE_WIDTH]
                .copy_from_slice(&row.iter().map(|elem| elem.lift()).collect_vec());

            // Is the current row's address different from the previous row's address?
            // Different: update running evaluation of Evaluation Argument with Program Table.
            // Not different: update running product of Permutation Argument with Processor Table.
            let mut is_duplicate_row = false;
            if let Some(prow) = previous_row {
                if prow[usize::from(Address)] == row[usize::from(Address)] {
                    is_duplicate_row = true;
                    debug_assert_eq!(prow[usize::from(CI)], row[usize::from(CI)]);
                    debug_assert_eq!(prow[usize::from(NIA)], row[usize::from(NIA)]);
                } else {
                    debug_assert_eq!(
                        prow[usize::from(Address)] + BFieldElement::one(),
                        row[usize::from(Address)]
                    );
                }
            }

            // Compress values of current row for Permutation Argument with Processor Table
            let ip = row[usize::from(Address)].lift();
            let ci = row[usize::from(CI)].lift();
            let nia = row[usize::from(NIA)].lift();
            let compressed_row_for_permutation_argument = ip * challenges.ip_processor_weight
                + ci * challenges.ci_processor_weight
                + nia * challenges.nia_processor_weight;

            // Update running product if same row has been seen before and not padding row
            if is_duplicate_row && row[usize::from(IsPadding)].is_zero() {
                processor_table_running_product *= challenges.processor_perm_indeterminate
                    - compressed_row_for_permutation_argument;
            }
            extension_row[usize::from(RunningProductPermArg)] = processor_table_running_product;

            // Compress values of current row for Evaluation Argument with Program Table
            let compressed_row_for_evaluation_argument = ip * challenges.address_weight
                + ci * challenges.instruction_weight
                + nia * challenges.next_instruction_weight;

            // Update running evaluation if same row has _not_ been seen before and not padding row
            if !is_duplicate_row && row[usize::from(IsPadding)].is_zero() {
                program_table_running_evaluation = program_table_running_evaluation
                    * challenges.program_eval_indeterminate
                    + compressed_row_for_evaluation_argument;
            }
            extension_row[usize::from(RunningEvaluation)] = program_table_running_evaluation;

            previous_row = Some(row.clone());
            extension_matrix.push(extension_row.to_vec());
        }

        assert_eq!(self.data().len(), extension_matrix.len());
        let padded_height = extension_matrix.len();
        let inherited_table = self.extension(
            extension_matrix,
            interpolant_degree,
            padded_height,
            ExtInstructionTable::ext_initial_constraints(challenges),
            ExtInstructionTable::ext_consistency_constraints(challenges),
            ExtInstructionTable::ext_transition_constraints(challenges),
            ExtInstructionTable::ext_terminal_constraints(challenges),
        );
        ExtInstructionTable { inherited_table }
    }

    pub fn for_verifier(
        interpolant_degree: Degree,
        padded_height: usize,
        all_challenges: &AllChallenges,
    ) -> ExtInstructionTable {
        let inherited_table = Table::new(
            BASE_WIDTH,
            FULL_WIDTH,
            vec![],
            "ExtInstructionTable".to_string(),
        );
        let base_table = Self { inherited_table };
        let empty_matrix: Vec<Vec<XFieldElement>> = vec![];
        let extension_table = base_table.extension(
            empty_matrix,
            interpolant_degree,
            padded_height,
            ExtInstructionTable::ext_initial_constraints(
                &all_challenges.instruction_table_challenges,
            ),
            ExtInstructionTable::ext_consistency_constraints(
                &all_challenges.instruction_table_challenges,
            ),
            ExtInstructionTable::ext_transition_constraints(
                &all_challenges.instruction_table_challenges,
            ),
            ExtInstructionTable::ext_terminal_constraints(
                &all_challenges.instruction_table_challenges,
            ),
        );

        ExtInstructionTable {
            inherited_table: extension_table,
        }
    }
}

impl ExtInstructionTable {
    pub fn to_fri_domain_table(
        &self,
        fri_domain: &FriDomain<XFieldElement>,
        omicron: XFieldElement,
        padded_height: usize,
        num_trace_randomizers: usize,
    ) -> Self {
        let ext_columns = self.base_width()..self.full_width();
        let fri_domain_codewords_ext = self.low_degree_extension(
            fri_domain,
            omicron,
            padded_height,
            num_trace_randomizers,
            ext_columns,
        );

        let inherited_table = self.inherited_table.with_data(fri_domain_codewords_ext);
        ExtInstructionTable { inherited_table }
    }
}

impl ExtensionTable for ExtInstructionTable {
    fn dynamic_initial_constraints(
        &self,
        challenges: &AllChallenges,
    ) -> Vec<MPolynomial<XFieldElement>> {
        ExtInstructionTable::ext_initial_constraints(&challenges.instruction_table_challenges)
    }

    fn dynamic_consistency_constraints(
        &self,
        challenges: &AllChallenges,
    ) -> Vec<MPolynomial<XFieldElement>> {
        ExtInstructionTable::ext_consistency_constraints(&challenges.instruction_table_challenges)
    }

    fn dynamic_transition_constraints(
        &self,
        challenges: &super::challenges::AllChallenges,
    ) -> Vec<MPolynomial<XFieldElement>> {
        ExtInstructionTable::ext_transition_constraints(&challenges.instruction_table_challenges)
    }

    fn dynamic_terminal_constraints(
        &self,
        challenges: &AllChallenges,
    ) -> Vec<MPolynomial<XFieldElement>> {
        ExtInstructionTable::ext_terminal_constraints(&challenges.instruction_table_challenges)
    }
}
