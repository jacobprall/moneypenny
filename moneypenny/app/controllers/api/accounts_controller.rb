class Api::AccountsController < ApplicationController
  before_action :deny_access_if_not_logged_in
  
  def index
    # @accounts = current_user.accounts
    render 'api/users/show'
  end

  def new
    @account = Account.new
  end

  def create
    @account = Account.create(account_params)
    if @account.save!
      render 'api/users/show'
    else
      render json: @account.errors.full_messages
    end
  end

  def edit
    @accounts = current_user.accounts
  end

  def update
    @account = Account.find(params[:id])
    if @account.update(account_params)
      render 'api/users/show'
    else
      render json: @account.errors.full_messages, status: 422
    end
  end

  def destroy
    @account = current_user.accounts.find(params[:id])
    @account.destroy
    render 'api/users/show'
  end

  def account_params
    params.require(:account).permit(:account_category, :institution, :label, :balance, :user_id)
  end
end

#
#  id               :bigint           not null, primary key
#  debit            :boolean          not null
#  account_category :string           not null
#  institution      :string           not null
#  label            :string           not null
#  balance          :float            not null
#  user_id          :integer          not null
#  created_at       :datetime         not null
#  updated_at       :datetime         not null