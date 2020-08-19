import {
  connect
} from 'react-redux';
import React from 'react';
import {
  updateAccount
} from '../../../actions/account_actions';
import {
  openModal,
  closeModal
} from '../../../actions/modal_actions';
import AccountForm from './account_form';

const mapStateToProps = (state, ownProps) => {
  return {
    errors: Object.values(state.errors.account),
    formType: 'edit',
    passedAccount: Object.assign(state.ui.modal.account[1], { user_id: state.session.id })
  };
};

const mapDispatchToProps = dispatch => {
  return {
    processForm: (account) => dispatch(updateAccount(account)),
    closeModal: () => dispatch(closeModal())
  };
};

export default connect(mapStateToProps, mapDispatchToProps)(AccountForm);
