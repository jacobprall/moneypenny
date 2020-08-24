
import React, {useState} from 'react'
import { useSelector, useDispatch, shallowEqual } from 'react-redux'
import {createAccount, deleteAccount, clearAccountErrors, updateAccount} from '../../../actions/account_actions'
import { closeModal } from '../../../actions/modal_actions'
import AccountForm from './account_form'

export default function account_form_container() {
  
  const selectedData = useSelector((state) => ({
    formType: state.ui.modal.formType[0],
    passedAccount: state.ui.modal.account[0],
    errors: Object.values(state.errors.account)
  }), shallowEqual);
  const dispatch = useDispatch();
  let processForm;
  if (selectedData.formType === 'new') {
    processForm = (account) => dispatch(createAccount(account));
  } else {
    processForm = (account) => dispatch(updateAccount(account))
  }

  const modalCloser = () => dispatch(closeModal());
  const accountErrorsClearer = () => dispatch(clearAccountErrors())
  const accountDeleter = (account) => (dispatch(deleteAccount(account)).then(() => modalCloser()))

  

  const props = {
    selectedData,
    processForm,
    modalCloser,
    accountErrorsClearer,
    accountDeleter
  }
  // console.log(props)

  return (
    <div className="modal-form-container">
      <AccountForm props={props} />
    </div>
  )
}

