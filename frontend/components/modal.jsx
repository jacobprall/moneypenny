import React from 'react';
import { closeModal } from '../actions/modal_actions';
import { connect } from 'react-redux';
// import AccountNewContainer from './accounts/account_form_modals/account_new_container';
// import AccountEditContainer from './accounts/account_form_modals/account_edit_container';
// import CreateTransactionContainer from './transactions/transaction_form/create_transaction_container'
// import EditTransactionContainer from './transactions/transaction_form/edit_transaction_container'

function Modal({ component, closeModal }) {
  
  if (!component.length) {
    return null;
  }

  const Component = component[0]
  
  return (
    <div className="modal-background" onClick={closeModal}>
      <div className="modal-child" onClick={e => e.stopPropagation()}>
        <Component />
      </div>
    </div>

  );
}


const mapStateToProps = state => {
  return {
    component: state.ui.modal.component
  };
};

const mapDispatchToProps = dispatch => {
  return {
    closeModal: () => dispatch(closeModal())
  };
};

export default connect(mapStateToProps, mapDispatchToProps)(Modal);