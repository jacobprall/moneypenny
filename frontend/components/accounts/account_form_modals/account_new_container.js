import {
  connect
} from 'react-redux';
import React from 'react';
import {
  postAccount
} from '../../../actions/account_actions';
import {
  openModal,
  closeModal
} from '../../../actions/modal_actions';
import AccountForm from './account_form';

const mapStateToProps = ({
  errors
}) => {
  return {
    errors: errors.session,
    formType: 'new',
    account: {account_category: "None", balance: 0, debit: true, institution: "None", label: ""}
  };
};

const mapDispatchToProps = dispatch => {
  return {
    processForm: (account) => dispatch(postAccount(account)),
    closeModal: () => dispatch(closeModal())
  };
};

export default connect(mapStateToProps, mapDispatchToProps)(AccountForm);
