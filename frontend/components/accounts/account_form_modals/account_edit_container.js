import {
  connect
} from 'react-redux';
import React from 'react';
import {
  patchAccount
} from '../../../actions/account_actions';
import {
  openModal,
  closeModal
} from '../../../actions/modal_actions';
import AccountForm from './account_form';

const mapStateToProps = (state, ownProps) => {
  return {
    errors: state.errors.session,
    formType: 'edit',
    account: ownProps.account
  };
};

const mapDispatchToProps = dispatch => {
  return {
    processForm: (account) => dispatch(patchAccount(account)),
    closeModal: () => dispatch(closeModal())
  };
};

export default connect(mapStateToProps, mapDispatchToProps)(AccountForm);
