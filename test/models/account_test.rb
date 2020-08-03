# == Schema Information
#
# Table name: accounts
#
#  id            :bigint           not null, primary key
#  account_type  :string           not null
#  balance       :decimal(8, 2)    not null
#  balance_sheet :string           not null
#  inst          :string
#  label         :string           not null
#  created_at    :datetime         not null
#  updated_at    :datetime         not null
#  user_id       :string           not null
#
# Indexes
#
#  index_accounts_on_user_id  (user_id)
#
require 'test_helper'

class AccountTest < ActiveSupport::TestCase
  # test "the truth" do
  #   assert true
  # end
end
