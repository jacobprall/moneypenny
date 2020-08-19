import React from 'react';
import { closeModal } from '../actions/modal_actions';
import { connect } from 'react-redux';
import AccountNewContainer from './accounts/account_form_modals/account_new_container';
import AccountEditContainer from './accounts/account_form_modals/account_edit_container';

function Modal({ modal, closeModal }) {
  if (!modal) {
    return null;
  }
  let component;
  switch (modal) {
    case 'new':
      component = <AccountNewContainer />;
      break;
    case 'edit':
      component = <AccountEditContainer />;
      break;
    default:
      return null;
  }
  return (
    <div className="modal-background" onClick={closeModal}>
      <div className="modal-child" onClick={e => e.stopPropagation()}>
        {component}
      </div>
    </div>
  );
}

const mapStateToProps = state => {
  return {
    modal: state.ui.modal.accounts
  };
};

const mapDispatchToProps = dispatch => {
  return {
    closeModal: () => dispatch(closeModal())
  };
};

export default connect(mapStateToProps, mapDispatchToProps)(Modal);