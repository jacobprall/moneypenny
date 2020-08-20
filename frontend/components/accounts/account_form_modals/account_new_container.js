import {
  connect
} from 'react-redux';
import React from 'react';
import {
  createAccount, clearAccountErrors
} from '../../../actions/account_actions';
import {
  closeModal
} from '../../../actions/modal_actions';
import AccountForm from './account_form';

const mapStateToProps = (state) => {
  return {
    errors: Object.values(state.errors.account),
    formType: 'new',
    passedAccount: {'account_category': "Cash", 'balance': 0, 'debit': false, 'institution': "None", 'label': "", 'user_id': `${state.session.id}`},

  };
};

const mapDispatchToProps = dispatch => {
  return {
    processForm: (account) => dispatch(createAccount(account)),
    closeModal: () => dispatch(closeModal()),
    clearAccountErrors: () => dispatch(clearAccountErrors()),
  };
};

export default connect(mapStateToProps, mapDispatchToProps)(AccountForm);
