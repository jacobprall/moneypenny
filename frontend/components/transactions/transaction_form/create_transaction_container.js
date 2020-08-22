import TransactionForm from './transaction_form'
import { connect } from 'react-redux'

import { createTransaction, clearTransactionErrors } from '../../../actions/transaction_actions'
import { closeModal } from '../../../actions/modal_actions'

const mSTP = (state) => ({
  
    errors: Object.values(state.errors.transaction),
    formType: 'new',
    passedTransaction: {
      'amount': 0,
      'date': new Date(),
      'description': 'None',
      'transaction_category': "Miscellaneous",
      'tags': "",
      'account_id': 0
    },

  
});

const mDTP = dispatch => ({
  processForm: transaction => dispatch(createTransaction(transaction)),
  closeModal: () => dispatch(closeModal()),
  clearTransactionErrors: () => dispatch(clearTransactionErrors())
});

export default connect(mSTP, mDTP)(TransactionForm)