import TransactionForm from './transaction_form'
import { connect } from 'react-redux'
import { updateTransaction, deleteTransaction, clearTransactionErrors } from '../../../actions/transaction_actions'
import { closeModal } from '../../../actions/modal_actions'
const mSTP = (state) => ({
  errors: state.errors.transaction,
  formType: 'edit',
  passedTransaction: Object.assign(state.ui.modal.transaction[1], {})
});

const mDTP = dispatch => ({
  processForm: (transaction) => dispatch(updateTransaction(transaction)),
  closeModal: () => dispatch(closeModal()),
  clearTransactionErrors: () => dispatch(clearAccountErrors()),
  deleteTransaction: (transaction) => dispatch(deleteTransaction(transaction))
});

export default connect(mSTP, mDTP)(TransactionForm)