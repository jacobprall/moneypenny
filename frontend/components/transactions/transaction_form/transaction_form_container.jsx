import React from 'react'
import { useDispatch, , shallowEqual, useSelector } from 'react-redux'
import TransactionForm from './transaction_form'
import {closeModal} from '../../../actions/modal_actions'
import { clearTransactionErrors, createTransaction, updateTransaction, deleteTransaction } from '../../../actions/transaction_actions'


export default function transaction_form_container() {

  const selectedData = useSelector((state) => ({
    errors: Object.values(state.errors.transaction),
    formType: state.ui.modal.formType[0],
    passedTransaction: state.ui.modal.transaction[0],
    accounts: state.entities.accounts
  }), shallowEqual);
  // console.log(selectedData)
  const dispatch = useDispatch();

  let processForm;
  if (selectedData.formType === 'new') {
    processForm = (transaction) => dispatch(createTransaction(transaction));
  } else {
    processForm = (transaction) => dispatch(updateTransaction(transaction)); 
  };
  
  const modalCloser = () => dispatch(closeModal());
  const transactionDeleter = (transaction) => (dispatch(deleteTransaction(transaction)).then(() => modalCloser()))
  const transactionErrorsClearer = () => dispatch(clearTransactionErrors());

  const props = {
    selectedData,
    processForm,
    modalCloser,
    transactionErrorsClearer,
    transactionDeleter
  }


  return (
    <div className="modal-form-container">
      <TransactionForm props={props} />
    </div>
  )
}
